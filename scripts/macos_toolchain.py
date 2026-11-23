#!/usr/bin/env python3
"""Keep local and CI builds on the same macOS appearance SDK."""
import argparse
import json
import os
from pathlib import Path
import re
import subprocess
import sys

POLICY = Path(__file__).with_name('macos-toolchain.json')


def version(value):
    if not re.fullmatch(r'\d+(?:\.\d+){0,2}', value):
        raise ValueError(f'Invalid macOS version: {value!r}')
    parts = tuple(map(int, value.split('.')))
    return parts + (0,) * (3 - len(parts))


def policy():
    return json.loads(POLICY.read_text())


def output(*command, env=None):
    return subprocess.check_output(command, env=env, text=True).strip()


def build_environment(*, github_actions=False):
    expected = policy()
    env = dict(os.environ)
    if github_actions:
        developer = Path(f'/Applications/Xcode_{expected["xcode"]}.app/Contents/Developer')
        if not developer.is_dir():
            raise ValueError(f'Pinned Xcode is missing: {developer}')
        env['DEVELOPER_DIR'] = str(developer)
        # A runner may already export an SDK from its default Xcode.
        env.pop('SDKROOT', None)
        actual = output('xcodebuild', '-version', env=env).splitlines()[0]
        if actual != f'Xcode {expected["xcode"]}':
            raise ValueError(f'Expected Xcode {expected["xcode"]}, found {actual}')
    sdk = env.get('SDKROOT') or 'macosx'
    actual = output('xcrun', '--sdk', sdk, '--show-sdk-version', env=env)
    if version(actual) != version(expected['sdk']):
        raise ValueError(
            f'Jam requires macOS SDK {expected["sdk"]}. Selected SDK is {actual}. '
            f'Select Xcode {expected["xcode"]} or matching Command Line Tools. '
            'The SDK controls system appearance, independently of the deployment target.')
    env['SDKROOT'] = output('xcrun', '--sdk', sdk, '--show-sdk-path', env=env)
    env['MACOSX_DEPLOYMENT_TARGET'] = expected['deployment_target']
    return env


def verify_build_metadata(metadata):
    expected = policy()
    builds = re.findall(r'^\s*platform\s+(\S+)\s*\n\s*minos\s+(\S+)\s*\n\s*sdk\s+(\S+)',
                        metadata, re.MULTILINE)
    if not builds:
        raise ValueError('The app has no readable LC_BUILD_VERSION metadata')
    for platform, minimum, sdk in builds:
        if (platform != 'MACOS' or version(sdk) != version(expected['sdk'])
                or version(minimum) != version(expected['deployment_target'])):
            raise ValueError(
                f'App build mismatch: {platform}, SDK {sdk}, minimum {minimum}. '
                f'Expected MACOS, SDK {expected["sdk"]}, minimum {expected["deployment_target"]}')


def verify_app(binary):
    verify_build_metadata(output('xcrun', 'vtool', '-show-build', str(binary)))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--github-actions', action='store_true')
    parser.add_argument('--binary', type=Path, help='Verify an already built app executable')
    parser.add_argument('command', nargs=argparse.REMAINDER, help='Run with the verified toolchain')
    args = parser.parse_args()
    try:
        if args.binary:
            verify_app(args.binary)
            print(f'Verified macOS build metadata: {args.binary}')
            return
        env = build_environment(github_actions=args.github_actions)
        expected = policy()
        print(f'macOS SDK {expected["sdk"]}, deployment target {expected["deployment_target"]}, '
              f'{env["SDKROOT"]}', flush=True)
        if args.github_actions:
            with open(os.environ['GITHUB_ENV'], 'a') as exports:
                for name in ('DEVELOPER_DIR', 'SDKROOT', 'MACOSX_DEPLOYMENT_TARGET'):
                    exports.write(f'{name}={env[name]}\n')
        command = args.command
        if command and command[0] == '--':
            command = command[1:]
        if command:
            sys.exit(subprocess.call(command, env=env))
    except (ValueError, OSError, subprocess.CalledProcessError) as error:
        parser.exit(1, f'{error}\n')


if __name__ == '__main__':
    main()
