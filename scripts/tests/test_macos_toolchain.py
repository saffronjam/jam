import importlib.util
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest
from unittest.mock import patch

SCRIPT = Path(__file__).resolve().parents[1] / 'macos_toolchain.py'
BUILD_SCRIPT = SCRIPT.parent / 'release/build-macos.sh'
spec = importlib.util.spec_from_file_location('macos_toolchain', SCRIPT)
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)

EXPECTED = {'xcode': '26.5', 'sdk': '26.5', 'deployment_target': '14.0'}


def metadata(sdk='26.5', minimum='14.0', platform='MACOS'):
    return f'''example.app/Contents/MacOS/Example:
Load command 11
      cmd LC_BUILD_VERSION
  cmdsize 32
 platform {platform}
    minos {minimum}
      sdk {sdk}
   ntools 1
     tool LD
  version 1267.0
'''


class ToolchainTests(unittest.TestCase):
    def setUp(self):
        patcher = patch.object(module, 'policy', return_value=EXPECTED)
        patcher.start()
        self.addCleanup(patcher.stop)

    def test_clt_sdk_is_accepted_and_deployment_target_is_preserved(self):
        with patch.dict(os.environ, {}, clear=True), patch.object(
                module, 'output', side_effect=['26.5', '/Library/Developer/SDKs/MacOSX26.5.sdk']):
            env = module.build_environment()
        self.assertEqual(env['SDKROOT'], '/Library/Developer/SDKs/MacOSX26.5.sdk')
        self.assertEqual(env['MACOSX_DEPLOYMENT_TARGET'], '14.0')
        self.assertNotIn('DEVELOPER_DIR', env)

    def test_legacy_sdk_is_rejected_before_building(self):
        with patch.dict(os.environ, {}, clear=True), patch.object(module, 'output', return_value='15.5'):
            with self.assertRaisesRegex(ValueError, 'requires macOS SDK 26.5'):
                module.build_environment()

    def test_explicit_sdk_override_is_checked(self):
        with patch.dict(os.environ, {'SDKROOT': '/old-sdk'}, clear=True), patch.object(
                module, 'output', return_value='15.5') as run:
            with self.assertRaises(ValueError):
                module.build_environment()
        self.assertEqual(run.call_args.args, ('xcrun', '--sdk', '/old-sdk', '--show-sdk-version'))

    def test_ci_selects_pinned_xcode_instead_of_runner_defaults(self):
        with patch.dict(os.environ, {'DEVELOPER_DIR': '/old-xcode', 'SDKROOT': '/old-sdk'}, clear=True), \
                patch.object(Path, 'is_dir', return_value=True), patch.object(
                    module, 'output', side_effect=['Xcode 26.5\nBuild version 17F42', '26.5', '/new-sdk']) as run:
            env = module.build_environment(github_actions=True)
        self.assertEqual(env['DEVELOPER_DIR'], '/Applications/Xcode_26.5.app/Contents/Developer')
        self.assertEqual(env['SDKROOT'], '/new-sdk')
        self.assertEqual(run.call_args_list[1].args, ('xcrun', '--sdk', 'macosx', '--show-sdk-version'))

    def test_missing_pinned_xcode_is_an_error(self):
        with patch.object(Path, 'is_dir', return_value=False), patch.object(module, 'output') as run:
            with self.assertRaisesRegex(ValueError, 'Pinned Xcode is missing'):
                module.build_environment(github_actions=True)
        run.assert_not_called()

    def test_matching_app_build_is_accepted(self):
        module.verify_build_metadata(metadata())
        module.verify_build_metadata(metadata(sdk='26.5.0', minimum='14.0.0'))

    def test_stale_app_is_rejected_even_with_new_toolchain(self):
        with self.assertRaisesRegex(ValueError, 'App build mismatch'):
            module.verify_build_metadata(metadata(sdk='15.5'))

    def test_raising_minimum_os_is_rejected(self):
        with self.assertRaisesRegex(ValueError, 'minimum 26.0'):
            module.verify_build_metadata(metadata(minimum='26.0'))

    def test_every_architecture_must_match(self):
        with self.assertRaises(ValueError):
            module.verify_build_metadata(metadata() + metadata(sdk='15.5'))

    def test_missing_or_wrong_platform_metadata_is_rejected(self):
        for value in ('', 'sdk 26.5', metadata(platform='IOS')):
            with self.subTest(value=value), self.assertRaises(ValueError):
                module.verify_build_metadata(value)

    def test_release_builder_preserves_fixture_flags_through_toolchain_selection(self):
        with tempfile.TemporaryDirectory(prefix='jam-build-arguments-') as directory:
            root = Path(directory)
            (root / 'scripts/release').mkdir(parents=True)
            shutil.copyfile(BUILD_SCRIPT, root / 'scripts/release/build-macos.sh')
            (root / 'scripts/bootstrap.py').write_text('pass\n')
            # Re-enter the real shell builder, without selecting a host SDK.
            (root / 'scripts/macos_toolchain.py').write_text(
                'import os, sys\n'
                'assert sys.argv[1] == "--"\n'
                'os.execvp(sys.argv[2], sys.argv[2:])\n')
            binaries = root / 'bin'
            binaries.mkdir()
            cargo = binaries / 'cargo'
            cargo.write_text(
                '#!/usr/bin/env python3\n'
                'import json, os, pathlib, sys\n'
                'pathlib.Path(os.environ["BUILD_ARGUMENTS_LOG"]).write_text('
                'json.dumps(sys.argv[1:]))\n'
                'sys.exit(37)\n')
            cargo.chmod(0o700)
            log = root / 'arguments.json'
            env = dict(os.environ, HOME=str(root), PATH=f'{binaries}:{os.environ["PATH"]}',
                       BUILD_ARGUMENTS_LOG=str(log))
            for arguments, features in (([], []), (['--fixture-services'],
                                                  ['--features', 'jam-core/fixture-services'])):
                with self.subTest(arguments=arguments):
                    log.unlink(missing_ok=True)
                    result = subprocess.run(
                        ['bash', str(root / 'scripts/release/build-macos.sh'), *arguments],
                        env=env, capture_output=True, text=True, timeout=10)
                    self.assertEqual(result.returncode, 37, result.stderr)
                    self.assertEqual(json.loads(log.read_text()),
                                     ['build', '--release', '--workspace', '--locked', *features])
