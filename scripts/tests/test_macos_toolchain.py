import importlib.util
import os
from pathlib import Path
import unittest
from unittest.mock import patch

SCRIPT = Path(__file__).resolve().parents[1] / 'macos_toolchain.py'
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
