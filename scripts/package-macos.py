#!/usr/bin/env python3
"""Stage, seal and atomically publish a complete macOS application bundle."""
import argparse
import ctypes
import fcntl
import json
import os
import pathlib
import plistlib
import shutil
import subprocess
import tempfile
import tomllib
from macos_toolchain import verify_app

ROOT = pathlib.Path(__file__).resolve().parent.parent


def bundle_in_use(bundle):
    """Keep code and resources addressable until every user has exited."""
    try:
        result = subprocess.run(['lsof', '-nP', '-t', '+D', str(bundle)], capture_output=True, timeout=15)
    except (OSError, subprocess.TimeoutExpired):
        return True  # An uncertain probe must never delete a live bundle.
    return bool(result.stdout.strip() or result.stderr.strip()) or result.returncode not in (0, 1)


def retired_directory(destination):
    return destination.parent / f'.{destination.name}.retired'


def prune_retired_bundles(destination):
    directory = retired_directory(destination)
    if not directory.exists():
        return
    for entry in directory.iterdir():
        if entry.name.startswith('bundle-') and entry.is_dir() and not entry.is_symlink():
            if not bundle_in_use(entry):
                shutil.rmtree(entry)


def retain_running_bundle(staged, destination):
    if not bundle_in_use(staged):
        return None
    directory = retired_directory(destination)
    directory.mkdir(mode=0o700, exist_ok=True)
    retained = pathlib.Path(tempfile.mkdtemp(prefix='bundle-', dir=directory)) / destination.name
    os.replace(staged, retained)
    return retained


def publish_bundle(staged, destination):
    """Swap whole directories; never rewrite a running executable's inode."""
    if not destination.exists():
        os.replace(staged, destination)
        return
    libc = ctypes.CDLL(None, use_errno=True)
    swap = libc.renamex_np
    swap.argtypes = [ctypes.c_char_p, ctypes.c_char_p, ctypes.c_uint]
    swap.restype = ctypes.c_int
    # Darwin sys/stdio.h: RENAME_SWAP, atomic on the same filesystem.
    if swap(os.fsencode(staged), os.fsencode(destination), 0x00000002):
        error = ctypes.get_errno()
        raise OSError(error, os.strerror(error), str(destination))
    return retain_running_bundle(staged, destination)


def assemble(staged, release):
    profile = 'release' if release else 'debug'
    version = tomllib.loads((ROOT / 'Cargo.toml').read_text())['workspace']['package']['version']
    contents = staged / 'Contents'
    files = {
        f'apps/macos/.build/{profile}/Jam': 'MacOS/Jam',
        f'target/{profile}/jamd': 'MacOS/jamd',
        # Jam and jam cannot share a directory on case-insensitive macOS disks.
        f'target/{profile}/jam': 'Resources/bin/jam',
        'skills/jam/SKILL.md': 'Resources/bin/SKILL.md',
        'assets/fonts/OFL.txt': 'Resources/licenses/JetBrainsMono-OFL.txt',
        'apps/macos/Info.plist': 'Info.plist',
    }
    for source in (ROOT / 'assets/agent-icons').glob('*'):
        files[str(source.relative_to(ROOT))] = 'Resources/agent-icons/' + source.name
    for source, relative in files.items():
        target = contents / relative
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(ROOT / source, target)
    # Check the executable, not only the active SDK: stale binaries may have
    # been linked against the old appearance even after selecting new tools.
    verify_app(contents / 'MacOS/Jam')
    remote = ROOT / 'build/remote'
    if remote.exists():
        destination = contents / 'Resources/remote'
        destination.mkdir(parents=True, exist_ok=True)
        for artifact in [*remote.glob('*.tar.gz'), *remote.glob('*.json')]:
            shutil.copy2(artifact, destination / artifact.name)
    plist = plistlib.loads((contents / 'Info.plist').read_bytes())
    plist['CFBundleShortVersionString'] = version
    major, minor, patch = map(int, version.split('.'))
    build_number = major * 1_000_000 + minor * 1_000 + patch
    if not (0 <= minor < 1000 and 0 <= patch < 1000 and build_number > 0):
        raise ValueError('Release version cannot be represented as a positive bundle build number')
    plist['CFBundleVersion'] = str(build_number)
    plist['CFBundleIconFile'] = 'Jam'
    (contents / 'Info.plist').write_bytes(plistlib.dumps(plist))
    if release:
        (contents / 'Resources/release.json').write_text(json.dumps({
            'version': version, 'repository': 'saffronjam/jam', 'platform': 'macos', 'arch': 'aarch64'
        }) + '\n')
        notices = ROOT / 'build/DEPENDENCIES.txt'
        if not notices.is_file():
            raise RuntimeError('Generate dependency notices before packaging a release')
        shutil.copy2(notices, contents / 'Resources/licenses/DEPENDENCIES.txt')
        shutil.copy2(ROOT / 'LICENSE', contents / 'Resources/licenses/Jam-LICENSE.txt')
        for arch in ('aarch64', 'x86_64'):
            if not (contents / f'Resources/remote/{arch}.tar.gz').is_file():
                raise RuntimeError(f'Missing Linux {arch} release artifact')
    iconset = staged.parent / 'Jam.iconset'
    subprocess.run(['swift', str(ROOT / 'scripts/release/icon.swift'), str(ROOT / 'assets/jam.svg'), str(iconset)], check=True)
    subprocess.run(['iconutil', '-c', 'icns', str(iconset), '-o', str(contents / 'Resources/Jam.icns')], check=True)
    # Copied signatures cannot seal this assembled app. Every build needs a
    # fresh resource seal after all executables, resources and plist changes.
    for binary in ('MacOS/Jam', 'MacOS/jamd', 'Resources/bin/jam'):
        subprocess.run(['codesign', '--force', '--sign', '-', str(contents / binary)], check=True)
    subprocess.run(['codesign', '--force', '--sign', '-', str(staged)], check=True)
    subprocess.run(['codesign', '--verify', '--deep', '--strict', str(staged)], check=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--release', action='store_true')
    args = parser.parse_args()
    bundle = ROOT / ('build/packaging/Jam.app' if args.release else 'build/Jam.app')
    bundle.parent.mkdir(parents=True, exist_ok=True)
    # Serializes independent builds, including the existence/swap decision.
    with (bundle.parent / '.jam-package.lock').open('a') as lock:
        fcntl.flock(lock, fcntl.LOCK_EX)
        prune_retired_bundles(bundle)
        with tempfile.TemporaryDirectory(prefix='.jam-package-', dir=bundle.parent) as directory:
            staged = pathlib.Path(directory) / 'Jam.app'
            assemble(staged, args.release)
            publish_bundle(staged, bundle)
            # Live bundles have moved out of staging. Their executable paths
            # and resources survive until a later build sees no remaining users.
        subprocess.run(['codesign', '--verify', '--deep', '--strict', str(bundle)], check=True)
    print(f'Built and verified {bundle}')


if __name__ == '__main__':
    main()
