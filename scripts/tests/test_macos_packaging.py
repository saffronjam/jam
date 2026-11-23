import importlib.util
import os
from pathlib import Path
import plistlib
import shutil
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

SCRIPT = Path(__file__).resolve().parents[1] / 'package-macos.py'
spec = importlib.util.spec_from_file_location('package_macos', SCRIPT)
module = importlib.util.module_from_spec(spec)
with patch.object(sys, 'path', [str(SCRIPT.parent), *sys.path]):
    spec.loader.exec_module(module)


@unittest.skipUnless(sys.platform == 'darwin', 'macOS atomic bundle publication')
class BundlePublicationTests(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory(prefix='jam-package-test-')
        self.addCleanup(self.directory.cleanup)
        self.root = Path(self.directory.name)
        source = self.root / 'fixture.c'
        source.write_text('#include <unistd.h>\nint main(void) { write(1,"ready\\n",6); for (;;) pause(); }\n')
        self.bundle = self.root / 'Jam.app'
        binary = self.bundle / 'Contents/MacOS/Jam'
        binary.parent.mkdir(parents=True)
        subprocess.run(['xcrun', 'clang', str(source), '-o', str(binary)], check=True)
        resources = self.bundle / 'Contents/Resources'
        resources.mkdir()
        (resources / 'marker.txt').write_text('old')
        (self.bundle / 'Contents/Info.plist').write_bytes(plistlib.dumps({
            'CFBundleIdentifier': 'dev.jam.packaging-test', 'CFBundleExecutable': 'Jam',
            'CFBundlePackageType': 'APPL', 'CFBundleVersion': '1'}))
        self.sign(self.bundle)

    def sign(self, bundle):
        subprocess.run(['codesign', '--force', '--sign', '-', str(bundle / 'Contents/MacOS/Jam')], check=True, capture_output=True)
        subprocess.run(['codesign', '--force', '--sign', '-', str(bundle)], check=True, capture_output=True)

    def verify(self, bundle):
        subprocess.run(['codesign', '--verify', '--deep', '--strict', str(bundle)], check=True, capture_output=True)

    def test_atomic_swap_preserves_running_executable_and_sealed_resources(self):
        binary = self.bundle / 'Contents/MacOS/Jam'
        process = subprocess.Popen([str(binary)], stdout=subprocess.PIPE)
        try:
            self.assertEqual(process.stdout.readline(), b'ready\n')
            with binary.open('rb') as old_file:
                old_stat = os.fstat(old_file.fileno())
                old_bytes = old_file.read()
                staged = self.root / 'staged/Jam.app'
                shutil.copytree(self.bundle, staged)
                (staged / 'Contents/Resources/marker.txt').write_text('new')
                self.sign(staged)
                retained = module.publish_bundle(staged, self.bundle)
                self.assertIsNone(process.poll(), 'Atomic packaging must not stop a running app')
                self.assertIsNotNone(retained)
                self.assertEqual((retained / 'Contents/MacOS/Jam').stat().st_ino, old_stat.st_ino)
                old_file.seek(0)
                self.assertEqual(old_file.read(), old_bytes, 'Mapped executable bytes must never change')
                self.assertEqual((self.bundle / 'Contents/Resources/marker.txt').read_text(), 'new')
                self.verify(self.bundle)
                self.verify(retained)
                module.prune_retired_bundles(self.bundle)
                self.assertTrue(retained.exists(), 'Live bundles must survive cleanup')
        finally:
            process.terminate()
            process.wait(timeout=5)
            process.stdout.close()
        module.prune_retired_bundles(self.bundle)
        self.assertFalse(retained.exists(), 'Unused bundles must be reclaimed')

    def test_uncertain_usage_keeps_old_bundle(self):
        with patch.object(module.subprocess, 'run', side_effect=OSError('unavailable')):
            self.assertTrue(module.bundle_in_use(self.bundle))

    def test_failed_swap_leaves_existing_bundle_valid(self):
        inode = (self.bundle / 'Contents/MacOS/Jam').stat().st_ino
        with self.assertRaises(OSError):
            module.publish_bundle(self.root / 'missing.app', self.bundle)
        self.assertEqual((self.bundle / 'Contents/MacOS/Jam').stat().st_ino, inode)
        self.verify(self.bundle)

    def test_initial_publish_moves_complete_bundle(self):
        destination = self.root / 'First.app'
        module.publish_bundle(self.bundle, destination)
        self.assertFalse(self.bundle.exists())
        self.verify(destination)
