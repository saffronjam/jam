"""Exercise the actual Rust build script without building the terminal engine."""

import os
from pathlib import Path
import subprocess
import tempfile
import unittest


class BuildIdentityTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.directory = tempfile.TemporaryDirectory(prefix='jam-build-identity-')
        cls.addClassCleanup(cls.directory.cleanup)
        cls.root = Path(cls.directory.name)
        cls.program = cls.root / 'build-id'
        source = Path(__file__).resolve().parents[2] / 'crates/jam-core/build.rs'
        subprocess.run([os.environ.get('RUSTC', 'rustc'), str(source), '-o', str(cls.program)],
                       check=True, capture_output=True, timeout=30)
        for name, content in {
            'Cargo.toml': 'same package', 'Cargo.lock': 'same dependencies',
            'crates/jam-core/src/lib.rs': 'same core', 'crates/jam/src/main.rs': 'same CLI',
        }.items():
            path = cls.root / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(content)

    def identity(self, folder, ca=b'same CA', key=b'same public key', origin='https://localhost:49651'):
        trust = self.root / folder
        trust.mkdir(exist_ok=True)
        (trust / 'ca.pem').write_bytes(ca)
        (trust / 'key.hex').write_bytes(key)
        result = subprocess.run(
            [str(self.program)], cwd=self.root / 'crates/jam-core',
            env=dict(os.environ, CARGO_FEATURE_FIXTURE_SERVICES='1',
                     JAM_FIXTURE_ORIGIN=origin, JAM_FIXTURE_CA_FILE=str(trust / 'ca.pem'),
                     JAM_FIXTURE_UPDATE_KEY_FILE=str(trust / 'key.hex')),
            check=True, capture_output=True, text=True, timeout=5,
        )
        return next(line for line in result.stdout.splitlines()
                    if line.startswith('cargo:rustc-env=JAM_BUILD_ID='))

    def test_same_source_and_trust_match_across_builder_paths(self):
        self.assertEqual(self.identity('linux-builder'), self.identity('macos-builder'))

    def test_trust_and_origin_changes_still_change_the_identity(self):
        original = self.identity('baseline')
        for changes in ({'ca': b'different CA'}, {'key': b'different public key'},
                        {'origin': 'https://localhost:49652'}):
            with self.subTest(changes=changes):
                self.assertNotEqual(original, self.identity('changed', **changes))
