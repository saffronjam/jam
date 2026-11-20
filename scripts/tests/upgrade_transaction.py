#!/usr/bin/env python3
"""Exercise signed macOS bundle replacement and rollback with a private profile.

Run in the macOS release job with its signing key. It creates no real release,
starts no agents or production daemon, and deletes all signed fixture artifacts.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import shutil
import subprocess
import tempfile


def run(*args, **kwargs):
    return subprocess.run(args, check=True, **kwargs)


def test(cli, signing_key):
    if platform.system() != 'Darwin' or platform.machine() != 'arm64':
        raise RuntimeError('This integration test requires an Apple Silicon Mac')
    current = subprocess.check_output([str(cli), '--version'], text=True).strip().split()[-1]
    major, minor, patch = map(int, current.split('.'))
    candidate = f'{major}.{minor}.{patch + 1}'
    with tempfile.TemporaryDirectory(prefix='jam-upgrade-rollback-') as temporary:
        directory = Path(temporary)
        state, config, stage, transaction = [directory / name for name in ('state', 'config', 'stage', 'transaction')]
        for path in (state, config, stage, transaction):
            path.mkdir(mode=0o700)
        previous, incoming = directory / 'Jam.app', directory / 'incoming/Jam.app'
        for app, version in ((previous, current), (incoming, candidate)):
            (app / 'Contents/MacOS').mkdir(parents=True)
            (app / 'Contents/Resources/bin').mkdir(parents=True)
            (app / 'Contents/Resources/release.json').write_text(json.dumps({'version': version}))
            (app / 'Contents/Info.plist').write_text('''<?xml version="1.0"?>
<plist version="1.0"><dict>
<key>CFBundleIdentifier</key><string>dev.jam.rollback-fixture</string>
<key>CFBundleExecutable</key><string>Jam</string>
<key>CFBundlePackageType</key><string>APPL</string>
</dict></plist>''')
            shutil.copy('/usr/bin/true', app / 'Contents/MacOS/Jam')
        shutil.copy(cli, previous / 'Contents/Resources/bin/jam')
        source = directory / 'candidate.rs'
        source.write_text(r'''fn main() {
    let args: Vec<_> = std::env::args().collect();
    if args[1] == "--version" { println!("jam CANDIDATE_VERSION"); return; }
    if args[1] == "__upgrade-restore" {
        let state = std::env::var("JAM_STATE_DIR").unwrap();
        std::fs::write(format!("{state}/candidate-started"), "yes").unwrap();
        std::fs::write(format!("{state}/layout.json"), "unreadable migrated layout").unwrap();
        eprintln!("Simulated failed new daemon health check");
        std::process::exit(1);
    }
    std::process::exit(2);
}'''.replace('CANDIDATE_VERSION', candidate))
        run('rustc', str(source), '-o', str(incoming / 'Contents/Resources/bin/jam'))
        for app in (previous, incoming):
            # Resource binaries must be signed explicitly before sealing the
            # bundle. Never execute an unsigned/invalid fixture on the host.
            run('codesign', '--force', '--sign', '-', str(app / 'Contents/Resources/bin/jam'), capture_output=True)
            run('codesign', '--force', '--deep', '--sign', '-', str(app), capture_output=True)
            run('codesign', '--verify', '--deep', '--strict', str(app))
            run('codesign', '--verify', '--strict', str(app / 'Contents/Resources/bin/jam'))
        archive = stage / f'Jam-v{candidate}-macos-arm64.zip'
        run('ditto', '-c', '-k', '--keepParent', str(incoming), str(archive))
        manifest = {'schema': 1, 'version': candidate, 'protocol': 'jam/fixture', 'min_macos': '14.0',
                    'assets': [{'name': archive.name, 'platform': 'macos', 'arch': 'aarch64',
                                'size': archive.stat().st_size, 'sha256': hashlib.sha256(archive.read_bytes()).hexdigest()}]}
        manifest_path = stage / 'manifest.json'
        manifest_path.write_text(json.dumps(manifest))
        signature = subprocess.check_output(['openssl', 'pkeyutl', '-sign', '-rawin', '-inkey',
                                             str(signing_key), '-in', str(manifest_path)])
        (stage / 'manifest.sig').write_text(signature.hex())
        (state / 'layout.json').write_text('original layout')
        (state / 'coordination.sqlite').write_text('original board')
        profile = {'JAM_STATE_DIR': str(state), 'JAM_SOCKET': str(directory / 'private.sock'),
                   'XDG_CONFIG_HOME': str(config), 'JAM_DEFAULTS_SUITE': 'dev.jam.rollback-fixture'}
        journal = {'schema': 1, 'token': 'f38c4cb9-e688-44e2-9969-858f8dd31a45',
                   'staged': {'directory': str(stage), 'release': manifest, 'archive': str(archive)},
                   'plan': {'installation': str(previous), 'local_running': False, 'targets': [],
                            'offline': [], 'app_pids': []},
                   'phase': 'staged', 'error': None, 'environment': profile}
        journal_path = transaction / 'journal.json'
        journal_path.write_text(json.dumps(journal))
        journal_path.chmod(0o600)
        environment = {key: value for key, value in os.environ.items() if key != 'JAM_RELEASE_SIGNING_KEY_PEM'}
        result = subprocess.run([str(previous / 'Contents/Resources/bin/jam'), '__upgrade-apply', str(journal_path)],
                                env={**environment, **profile}, capture_output=True, text=True, timeout=60)
        assert result.returncode != 0, result.stdout
        assert (state / 'candidate-started').exists(), f'Candidate never executed: {result.stderr}'
        assert json.loads((previous / 'Contents/Resources/release.json').read_text())['version'] == current
        assert (state / 'layout.json').read_text() == 'original layout'
        assert (state / 'coordination.sqlite').read_text() == 'original board'
        assert json.loads(journal_path.read_text())['phase'] in ('failed', 'rolled_back')
        print('Signed upgrade rollback passed: previous app, layout and board restored; private failure journal retained.')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--cli', type=Path, required=True)
    parser.add_argument('--signing-key', type=Path, required=True)
    args = parser.parse_args()
    test(args.cli.resolve(), args.signing_key.resolve())


if __name__ == '__main__':
    main()
