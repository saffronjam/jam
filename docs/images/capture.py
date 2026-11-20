#!/usr/bin/env python3
"""Capture Jam running real Claude/Codex jobs in a disposable Git worktree.

Uses existing agent authentication and makes small inference requests. No live
Jam sessions, repositories, or remotes are used. Quota labels are demo fixtures.
"""
import argparse
import json
import os
from pathlib import Path
import plistlib
import shlex
import shutil
import subprocess
import tempfile
import time
import uuid

ROOT = Path(__file__).resolve().parents[2]


def run(*command, **kwargs):
    return subprocess.run(command, check=True, text=True, **kwargs)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--app', type=Path, default=ROOT / 'build/Jam.app')
    parser.add_argument('--output', type=Path, default=Path(__file__).with_name('hero.png'))
    parser.add_argument('--timeout', type=float, default=180, help='Agent preparation deadline in seconds')
    parser.add_argument('--fixture-only', action='store_true', help='Offline UI fixture, without agent requests')
    args = parser.parse_args()
    if not args.app.is_dir():
        raise SystemExit('Build Jam first with just build, or pass --app')
    if args.timeout <= 0:
        raise SystemExit('--timeout must be positive')
    if not args.fixture_only:
        for name in ('claude', 'codex'):
            if not shutil.which(name):
                raise SystemExit(name + ' is required for the live documentation demo')
    output = args.output.resolve()
    output.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix='jam-demo-', dir='/tmp') as temporary:
        directory = Path(temporary)
        captured = directory / 'hero.png'
        app = directory / 'Jam Demo.app'
        shutil.copytree(args.app, app, symlinks=True)
        plist = app / 'Contents/Info.plist'
        data = plistlib.loads(plist.read_bytes())
        data['CFBundleIdentifier'] = 'dev.saffronjam.documentation-demo'
        plist.write_bytes(plistlib.dumps(data))
        run('codesign', '--force', '--sign', '-', str(app))
        run('codesign', '--verify', '--deep', '--strict', str(app))
        state_directory = directory / 'state'
        state_directory.mkdir(mode=0o700)
        env = dict(os.environ, JAM_SOCKET=str(directory / 'socket'), JAM_STATE_DIR=str(state_directory),
                   XDG_CONFIG_HOME=str(directory / 'config'), JAM_CAPTURE_ISOLATED='1',
                   JAM_DEFAULTS_SUITE='dev.saffronjam.fixture.capture.' + uuid.uuid4().hex)
        for name in ('JAM_PANE_ID', 'JAM_WORKSPACE_ID', 'JAM_AGENT_ID', 'JAM_COORDINATOR_ID'):
            env.pop(name, None)
        daemon = None
        cli = app / 'Contents/Resources/bin/jam'
        try:
            extra = []
            if not args.fixture_only:
                daemon, snapshot = prepare_agents(directory, app, cli, env, args.timeout)
                extra = ['--capture-state', str(snapshot)]
            command = ['open', '-n', '-W']
            for key in ('JAM_SOCKET', 'JAM_STATE_DIR', 'XDG_CONFIG_HOME', 'JAM_DEFAULTS_SUITE', 'JAM_CAPTURE_ISOLATED'):
                command += ['--env', key + '=' + env[key]]
            command += ['-a', str(app), '--args', '--preview', '--capture-demo', str(captured), *extra]
            run(*command, cwd=directory, timeout=35)
            if not captured.exists() or captured.stat().st_size < 1024 or captured.read_bytes()[:8] != b'\x89PNG\r\n\x1a\n':
                raise SystemExit('Jam did not produce a screenshot; requires macOS 14.4+ and prepared native frames')
            staged = output.with_suffix('.tmp.png')
            shutil.copy2(captured, staged)
            staged.replace(output)
        finally:
            if daemon is not None:
                try:
                    subprocess.run([str(cli), 'daemon', 'stop'], env=env, cwd=directory,
                                   stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, timeout=12, check=False)
                except subprocess.TimeoutExpired:
                    pass
                daemon.terminate()
                try:
                    daemon.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    daemon.kill()
                    daemon.wait()
            # Remove only this invocation's temporary preference suite.
            subprocess.run(['defaults', 'delete', env['JAM_DEFAULTS_SUITE']],
                           stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, check=False)
    print(output)


def prepare_agents(directory, app, cli, env, timeout):
    repository = directory / 'atlas'
    repository.mkdir()
    run('git', 'init', '-b', 'main', str(repository), stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    run('git', '-C', str(repository), 'config', 'user.name', 'Jam Demo')
    run('git', '-C', str(repository), 'config', 'user.email', 'demo@example.invalid')
    (repository / 'README.md').write_text('# Atlas\n\nA tiny terminal-search demo.\n')
    run('git', '-C', str(repository), 'add', 'README.md')
    run('git', '-C', str(repository), 'commit', '-m', 'Start terminal search demo', stdout=subprocess.DEVNULL)
    tools = directory / 'tools'
    tools.mkdir(mode=0o700)
    shell = tools / 'demo-shell'
    shell.write_text("#!/bin/sh\nexport BASH_SILENCE_DEPRECATION_WARNING=1\nPS1='❯ ' exec /bin/bash --noprofile --norc -i\n")
    shell.chmod(0o700)
    env['SHELL'] = str(shell)
    env['PATH'] = str(tools) + os.pathsep + env.get('PATH', '')
    # Keep the provider's shared credential store so token refresh stays coordinated.
    settings = directory / 'claude-settings.json'
    settings.write_text(json.dumps({'disableAllHooks': True}))
    common = ('This is a public documentation demo in a disposable repository. '
              'Work only in this repository. Do not read or modify outside files, secrets, account data, '
              'network services, remotes, or other checkouts. Do not commit or push. Keep work tiny and output concise.')
    claude_task = ('Implement search.py with a function search_lines(lines, query) that returns the lines '
                   'containing query, case-insensitively. Empty query returns all lines. '
                   'Use only Python standard library, keep the function under 12 lines, and run one small smoke test. '
                   'Finish with Search ready. and one short summary.')
    codex_task = ('Review search.py, write test_search.py with four focused unittest cases '
                  '(case-insensitive search, no match, empty query, and preserved order), and run the tests. '
                  'Do not change search.py unless a test reveals a bug. '
                  'Finish with Review complete. and a short test summary.')
    commands = {
        'demo-claude': [shutil.which('claude'), '--permission-mode', 'acceptEdits',
                        '--allowedTools', 'Read,Write,Edit,Glob,Grep,Bash(python3 *)', '--effort', 'low',
                        '--model', 'sonnet', '--setting-sources', '', '--settings', str(settings),
                        '--strict-mcp-config', '--mcp-config', '{"mcpServers":{}}',
                        '--disable-slash-commands', '--system-prompt', common,
                        '--name', 'Terminal search', claude_task],
        'demo-codex': [shutil.which('codex'), '--sandbox', 'workspace-write', '--ask-for-approval', 'never',
                       '--no-alt-screen', '-m', 'gpt-6.1-sol',
                       '--disable', 'hooks', '--disable', 'plugins', '--disable', 'remote_plugin',
                       '-c', 'project_doc_max_bytes=0', '-c', 'suppress_unstable_features_warning=true',
                       '-c', 'projects.' + json.dumps(str((directory / 'terminal-search').resolve())) + '.trust_level="trusted"',
                       '-c', 'projects.' + json.dumps(str(repository.resolve())) + '.trust_level="trusted"',
                       '-c', 'model_reasoning_effort="low"',
                       '-c', 'developer_instructions=' + json.dumps(common), codex_task],
    }
    for name, command in commands.items():
        path = tools / name
        path.write_text('#!/bin/sh\nexec ' + shlex.join(command) + '\n')
        path.chmod(0o700)
    daemon_log = (directory / 'daemon.log').open('w')
    daemon = subprocess.Popen([str(app / 'Contents/MacOS/jamd')], env=env, cwd=directory,
                              stdout=daemon_log, stderr=subprocess.STDOUT, start_new_session=True)
    deadline = time.monotonic() + timeout

    def jam(*args):
        result = run(str(cli), '--json', *args, env=env, cwd=directory,
                     stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=15)
        value = json.loads(result.stdout)
        if 'Error' in value or 'error' in value:
            raise RuntimeError('Private demo daemon rejected a request: ' + str(value))
        return value

    def wait(label, predicate):
        while not predicate():
            if daemon.poll() is not None:
                raise RuntimeError('Private demo daemon exited during ' + label)
            if time.monotonic() >= deadline:
                raise RuntimeError('Timed out preparing real demo agents: ' + label)
            time.sleep(0.2)

    try:
        wait('daemon startup', lambda: Path(env['JAM_SOCKET']).exists())
        state = jam('workspace', 'create', str(repository), '--name', 'Atlas')['State']
        root = state['workspaces'][0]
        jam('worktree', 'create', '--workspace', root['id'], '--branch', 'feature/terminal-search',
            '--path', str(directory / 'terminal-search'))
        state = jam('workspace', 'list')['State']
        worktree = next(w for w in state['workspaces'] if w.get('parent') == root['id'])
        pane = worktree['tabs'][0]['root']['Leaf']
        state = jam('pane', 'split', '--pane', pane, '--direction', 'right')['State']
        second = next(p['id'] for p in state['panes'] if p['workspace'] == worktree['id'] and p['id'] != pane)
        jam('pane', 'send', '--pane', pane, '--text', 'exec demo-claude', '--enter')
        worktree_path = Path(worktree['path'])
        trusted_claude = False
        def implementation_ready():
            nonlocal trusted_claude
            if (worktree_path / 'search.py').exists():
                return True
            screen = jam('pane', 'capture', '--pane', pane).get('Capture', '')
            if not trusted_claude and 'Accessing workspace:' in screen and 'Yes, I trust this folder' in screen:
                # This is the worktree just created above, never a user checkout.
                time.sleep(0.5)
                jam('pane', 'send', '--pane', pane, '--text', '\x1b[B')
                time.sleep(0.3)
                jam('pane', 'send', '--pane', pane, '--text', '', '--enter')
                trusted_claude = True
            return False
        wait('Claude implementation', implementation_ready)
        jam('pane', 'send', '--pane', second, '--text', 'exec demo-codex', '--enter')
        trusted_codex = False
        def review_ready():
            nonlocal trusted_codex
            if (worktree_path / 'test_search.py').is_file():
                result = subprocess.run(['python3', '-c',
                    'import unittest; s=unittest.defaultTestLoader.discover("."); '
                    'n=s.countTestCases(); r=unittest.TextTestRunner(verbosity=2).run(s); '
                    'raise SystemExit(0 if n >= 4 and r.wasSuccessful() else 1)'],
                    cwd=worktree_path, stdout=subprocess.DEVNULL, stderr=subprocess.PIPE, timeout=10)
                if result.returncode == 0:
                    return True
            screen = jam('pane', 'capture', '--pane', second).get('Capture', '')
            if not trusted_codex and 'Trust this folder?' in screen and '1. Trust and continue' in screen:
                # Codex resolves trust to the Git common root; both roots are ours.
                time.sleep(0.5)
                jam('pane', 'send', '--pane', second, '--text', '', '--enter')
                trusted_codex = True
            return False
        wait('Codex review and tests', review_ready)
        # Real test discovery/result is the readiness gate, independent of TUI wording.
        # Allow the agents to present their final summaries after the passing test run.
        time.sleep(5)
        for pane_id in (pane, second):
            jam('status', 'set', '--pane', pane_id, '--state', 'done')
        run('git', '-C', str(worktree_path), 'add', '-N', 'search.py', 'test_search.py')
        diff = run('git', '-C', str(worktree_path), 'diff', '--numstat', stdout=subprocess.PIPE).stdout
        counts = [line.split('\t')[:2] for line in diff.splitlines()]
        added = sum(int(a) for a, _ in counts if a.isdigit())
        removed = sum(int(b) for _, b in counts if b.isdigit())
        state = jam('workspace', 'list')['State']
        for w in state['workspaces']:
            if w['id'] == worktree['id']:
                w['name'] = 'Search'
                w['auto_name'] = False
                w['tabs'][0]['name'] = 'Implementation & Review'
        for p in state['panes']:
            if p['id'] in (pane, second):
                p['agent_provider'] = 'claude' if p['id'] == pane else 'codex'
                p['agent_role'] = 'worker' if p['id'] == pane else 'reviewer'
                p['title'] = 'Implementation' if p['id'] == pane else 'Review'
        snapshot = Path(env['JAM_STATE_DIR']) / 'capture.json'
        snapshot.write_text(json.dumps({'State': state, 'selected_workspace': worktree['id'],
                                        'git_summaries': {worktree['id']: {'added': added, 'removed': removed}}}))
        print('Prepared real Claude implementation and Codex review in a private Git worktree.', flush=True)
        return daemon, snapshot
    except BaseException:
        try:
            subprocess.run([str(cli), 'daemon', 'stop'], env=env, cwd=directory,
                           stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, timeout=12, check=False)
        except subprocess.TimeoutExpired:
            pass
        daemon.terminate()
        try:
            daemon.wait(timeout=5)
        except subprocess.TimeoutExpired:
            daemon.kill()
            daemon.wait()
        raise
    finally:
        daemon_log.close()


if __name__ == '__main__':
    main()
