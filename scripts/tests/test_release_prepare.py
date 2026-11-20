import hashlib
import importlib.util
import io
import json
from pathlib import Path
import shutil
import subprocess
import tarfile
import tempfile
import unittest
from unittest.mock import patch
import zipfile

SCRIPT = Path(__file__).resolve().parents[1] / 'release/prepare.py'
spec = importlib.util.spec_from_file_location('release_prepare', SCRIPT)
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)


@unittest.skipUnless(shutil.which('openssl'), 'OpenSSL required for release signatures')
class ReleaseTests(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.root = Path(self.directory.name)
        self.key = self.root / 'key.pem'
        subprocess.run(['openssl', 'genpkey', '-algorithm', 'Ed25519', '-out', str(self.key)], check=True, capture_output=True)
        public = subprocess.check_output(['openssl', 'pkey', '-in', str(self.key), '-pubout', '-outform', 'DER'])
        (self.root / 'assets').mkdir()
        (self.root / 'assets/update-public-key.hex').write_text(public[-32:].hex())
        (self.root / 'Cargo.toml').write_text('[workspace.package]\nversion="0.0.1"\n')
        (self.root / 'build/remote').mkdir(parents=True)
        (self.root / 'build/release').mkdir()
        for arch in ('aarch64', 'x86_64'):
            payload = {name: (name + arch).encode() for name in ('jam', 'jamd', 'SKILL.md')}
            manifest = {'release_version': '0.0.1', 'arch': arch, 'protocol': 'jam/test',
                        'files': {name: hashlib.sha256(data).hexdigest() for name, data in payload.items()}}
            payload['manifest.json'] = json.dumps(manifest).encode()
            (self.root / f'build/remote/{arch}.json').write_text(json.dumps(manifest))
            with tarfile.open(self.root / f'build/remote/{arch}.tar.gz', 'w:gz') as archive:
                for name, data in payload.items():
                    item = tarfile.TarInfo(name)
                    item.size = len(data)
                    archive.addfile(item, io.BytesIO(data))
        self.zip = self.root / 'build/release/Jam-v0.0.1-macos-arm64.zip'
        with zipfile.ZipFile(self.zip, 'w') as archive:
            archive.writestr('Jam.app/Contents/Resources/release.json', json.dumps(
                {'version': '0.0.1', 'platform': 'macos', 'arch': 'aarch64'}))

    def prepare(self, version='0.0.1'):
        with patch.object(module, 'ROOT', self.root):
            module.prepare(version, self.key)

    def test_manifest_matches_verified_artifacts_and_signature(self):
        self.prepare()
        output = self.root / 'build/release'
        manifest = json.loads((output / 'manifest.json').read_text())
        self.assertEqual(len(manifest['assets']), 3)
        for asset in manifest['assets']:
            data = (output / asset['name']).read_bytes()
            self.assertEqual(asset['sha256'], hashlib.sha256(data).hexdigest())
            self.assertEqual(asset['size'], len(data))
        signature = output / 'signature.bin'
        signature.write_bytes(bytes.fromhex((output / 'manifest.sig').read_text().strip()))
        pub = output / 'public.pem'
        subprocess.run(['openssl', 'pkey', '-in', str(self.key), '-pubout', '-out', str(pub)], check=True)
        subprocess.run(['openssl', 'pkeyutl', '-verify', '-rawin', '-pubin', '-inkey', str(pub),
                        '-in', str(output / 'manifest.json'), '-sigfile', str(signature)], check=True, capture_output=True)
        (output / 'manifest.json').write_text('{}')
        result = subprocess.run(['openssl', 'pkeyutl', '-verify', '-rawin', '-pubin', '-inkey', str(pub),
                                 '-in', str(output / 'manifest.json'), '-sigfile', str(signature)], capture_output=True)
        self.assertNotEqual(result.returncode, 0)

    def test_wrong_trust_key_refuses_signing(self):
        (self.root / 'assets/update-public-key.hex').write_text('00' * 32)
        with self.assertRaisesRegex(ValueError, 'trust key'):
            self.prepare()
        self.assertFalse((self.root / 'build/release/manifest.sig').exists())

    def test_tag_and_payload_versions_must_match(self):
        with self.assertRaisesRegex(ValueError, 'Cargo version'):
            self.prepare('0.0.2')
        metadata = self.root / 'build/remote/aarch64.json'
        data = json.loads(metadata.read_text())
        data['release_version'] = '0.0.2'
        metadata.write_text(json.dumps(data))
        with self.assertRaisesRegex(ValueError, 'metadata'):
            self.prepare()

    def test_modified_archive_cannot_be_signed(self):
        metadata = self.root / 'build/remote/aarch64.json'
        data = json.loads(metadata.read_text())
        data['files']['jam'] = '00' * 32
        metadata.write_text(json.dumps(data))
        with self.assertRaisesRegex(ValueError, 'differs'):
            self.prepare()

    def test_wrong_app_bundle_is_rejected(self):
        with zipfile.ZipFile(self.zip, 'w') as archive:
            archive.writestr('Jam.app/Contents/Resources/release.json', json.dumps(
                {'version': '0.0.2', 'platform': 'macos', 'arch': 'aarch64'}))
        with self.assertRaisesRegex(ValueError, 'marker'):
            self.prepare()
