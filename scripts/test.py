#!/usr/bin/env python3
"""Run Jam's tests. Live remote tests require an explicit remote name."""
import argparse
import os
from pathlib import Path
import platform
import re
import subprocess
import tempfile
import time

ROOT = Path(__file__).resolve().parent.parent


def run(*args):
    subprocess.run(args, cwd=ROOT, check=True)


def macos_tests():
    tests = sorted((ROOT / 'apps/macos/Tests/JamTests').glob('*Tests.swift'))
    if not tests:
        raise RuntimeError('No macOS tests discovered')
    names = [path.stem for path in tests]
    if any(not re.fullmatch(r'[A-Za-z_][A-Za-z0-9_]*', name) for name in names):
        raise RuntimeError('Invalid Swift test suite name')
    sources = ROOT / 'apps/macos/Sources/Jam'
    dependencies = [
        'LaunchDirectory.swift', 'SplitLayout.swift', 'Node.swift', 'RequestRouting.swift',
        'AppUpgradeState.swift',
        'PaneLayout.swift', 'ReorderSession.swift', 'WorkspaceCell.swift',
        'GitSummaryValue.swift', 'WorkspaceSelection.swift', 'WorkspaceName.swift', 'WarningToast.swift',
        'WorkspaceCreation.swift',
        'SidebarUpdates.swift',
        'TerminalCachePolicy.swift',
        'WorktreeChoices.swift',
        'ServerStatus.swift',
        'UsageState.swift', 'AgentModelSettings.swift',
    ]
    # Command Line Tools do not ship XCTest/Testing. Compile all existing
    # assertion suites into one discovered runner with assertions enabled.
    with tempfile.TemporaryDirectory(prefix='jam-tests-') as temporary:
        runner = Path(temporary) / 'TestRunner.swift'
        calls = '\n'.join(f'    try {name}.run()' for name in names)
        runner.write_text(
            'import Foundation\n@main struct TestRunner {\n'
            '  static func main() throws {\n' + calls + '\n  }\n}\n'
        )
        executable = Path(temporary) / 'jam-tests'
        run('xcrun', 'swiftc', '-Onone',
            *(str(sources / name) for name in dependencies),
            *(str(path) for path in tests), str(runner), '-o', str(executable))
        run(str(executable))


def macos_ui_tests():
    run('cargo', 'build', '--locked', '-p', 'jam-native', '-p', 'jam')
    sources = ROOT / 'apps/macos/Sources/Jam'
    with tempfile.TemporaryDirectory(prefix='jam-ui-', dir='/tmp') as temporary:
        directory = Path(temporary)
        module = directory / 'module.modulemap'
        module.write_text('module CJam { header "' + str(
            ROOT / 'apps/macos/Sources/CJam/include/jam.h') + '" export * }\n')
        app = directory / 'Jam.swift'
        app.write_text((sources / 'Jam.swift').read_text().replace(
            '@main struct JamApp', 'struct JamApp', 1))
        executable = directory / 'terminal-tests'
        frameworks = ['Metal', 'QuartzCore', 'CoreGraphics', 'CoreText', 'Security', 'SystemConfiguration']
        run('xcrun', 'swiftc', '-Onone', '-I', str(directory),
            '-L', str(ROOT / 'target/debug'), '-ljam_native', '-lc++',
            *(flag for name in frameworks for flag in ('-framework', name)),
            *(str(path) for path in sorted(sources.glob('*.swift')) if path.name != 'Jam.swift'),
            str(app), str(ROOT / 'apps/macos/Tests/JamUITests/TerminalSwitchTests.swift'),
            '-o', str(executable))
        state = directory / 'state'
        state.mkdir(mode=0o700)
        socket = directory / 'socket'
        # Keep login/profile customizations out of terminal lifecycle tests.
        shell = directory / 'test-shell'
        shell.write_text("#!/bin/sh\nPS1='jam-test> ' exec /bin/bash --noprofile --norc -i\n")
        shell.chmod(0o700)
        claude = directory / 'claude'
        claude.write_text("#!/bin/sh\nprintf '\\nJam fixture agent ready\\n'\nexec /bin/cat\n")
        claude.chmod(0o700)
        env = dict(os.environ, JAM_SOCKET=str(socket), JAM_STATE_DIR=str(state), SHELL=str(shell),
                   JAM_DAEMON=str(ROOT / 'target/debug/jamd'),
                   PATH=f'{directory}{os.pathsep}{os.environ["PATH"]}',
                   XDG_CONFIG_HOME=str(directory / 'config'),
                   JAM_DEFAULTS_SUITE='dev.saffronjam.fixture.' + directory.name)
        # Usage discovery can include configured logins without a running agent.
        # Use fixture-owned profile directories for every provider.
        env.update(CODEX_HOME=str(directory / 'codex'), CLAUDE_CONFIG_DIR=str(directory / 'claude-home'),
                   CLAUDE_SECURESTORAGE_CONFIG_DIR=str(directory / 'claude-home'))
        env.pop('CLAUDE_CODE_OAUTH_TOKEN', None)
        daemon = subprocess.Popen([str(ROOT / 'target/debug/jamd')], env=env)
        try:
            deadline = time.monotonic() + 10
            while not socket.exists():
                if daemon.poll() is not None or time.monotonic() >= deadline:
                    raise RuntimeError('Test daemon did not start')
                time.sleep(0.01)
            for suite in ('--controls-only', '--workspace-only'):
                subprocess.run([str(executable), '--preview', suite], cwd=ROOT, env=env, check=True, timeout=40)
        finally:
            # A lifecycle test may have replaced the original daemon process.
            # Stop whichever instance now owns this private fixture socket.
            try:
                subprocess.run([str(ROOT / 'target/debug/jam'), 'daemon', 'stop'],
                               env=env, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
                               timeout=5, check=False)
            except subprocess.TimeoutExpired:
                pass
            daemon.terminate()
            try:
                daemon.wait(timeout=5)
            except subprocess.TimeoutExpired:
                daemon.kill()
                daemon.wait()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('mode', nargs='?', choices=['all', 'core', 'remote'], default='all')
    parser.add_argument('remote', nargs='?', default='')
    args = parser.parse_args()
    if args.mode == 'remote':
        if not args.remote:
            parser.error('remote mode requires a saved remote name')
        run('cargo', 'build', '--locked', '-p', 'jam', '--bin', 'jam')
        run('python3', 'scripts/remote-smoke.py', args.remote)
        return
    if args.remote:
        parser.error('a remote name is only valid in remote mode')
    run('cargo', 'test', '--locked', *(['--workspace'] if args.mode == 'all' else ['-p', 'jam-core', '-p', 'jam']))
    run('python3', '-m', 'unittest', 'discover', '-s', 'scripts/tests', '-v')
    if platform.system() == 'Darwin':
        macos_tests()
        macos_ui_tests()
    cache = ROOT / '.cache'
    cache.mkdir(exist_ok=True)
    fixture = cache / 'local.snapshot'
    run('cargo', 'run', '--locked', '-p', 'jam-core', '--example', 'snapshot_fixture', '--', 'write', str(fixture))
    for path in [fixture, *sorted((ROOT / 'build/remote').glob('*.snapshot'))]:
        run('cargo', 'run', '--locked', '-p', 'jam-core', '--example', 'snapshot_fixture', '--', 'verify', str(path))


if __name__ == '__main__':
    main()
