"""Reject normal fixture applications bundling servers they cannot offer as updates."""

import argparse
import json
from pathlib import Path
import subprocess


def verify(bundle):
    info = json.loads(subprocess.run(
        [str(bundle / 'Contents/MacOS/jamd'), '--build-info'],
        check=True, capture_output=True, text=True, timeout=10,
    ).stdout)
    for arch in ('aarch64', 'x86_64'):
        remote = bundle / 'Contents/Resources/remote'
        metadata = json.loads((remote / (arch + '.json')).read_text())
        expected = (info['build'], info['protocol'], info['release_version'])
        actual = (metadata['server_build'], metadata['protocol'], metadata['release_version'])
        if actual != expected or metadata['arch'] != arch:
            raise RuntimeError(f'Bundled {arch} server identity {actual} does not match native {expected}')
        if not (remote / (arch + '.tar.gz')).is_file():
            raise RuntimeError(f'Bundled {arch} server archive is missing')
    print('Both bundled remote server identities match the native fixture application.')


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('bundle', type=Path)
    verify(parser.parse_args().bundle)
