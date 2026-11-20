#!/usr/bin/env python3
"""Validate release assets, create a manifest, and sign its exact bytes."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import tempfile
import tarfile
import zipfile
import tomllib

ROOT = Path(__file__).resolve().parents[2]


def prepare(version, key):
    configured = tomllib.loads((ROOT / 'Cargo.toml').read_text())['workspace']['package']['version']
    if not re.fullmatch(r'0|[1-9][0-9]*', version.split('.')[0]) or not re.fullmatch(r'(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)', version):
        raise ValueError('Release requires a stable semantic version')
    if version != configured:
        raise ValueError(f'Tag v{version} does not match Cargo version {configured}')
    output = ROOT / 'build/release'
    output.mkdir(parents=True, exist_ok=True)
    assets = []
    protocols = set()
    for arch in ('x86_64', 'aarch64'):
        source = ROOT / f'build/remote/{arch}.tar.gz'
        info = json.loads((source.parent / f'{arch}.json').read_text())
        if info['release_version'] != version or info['arch'] != arch:
            raise ValueError(f'Linux {arch} metadata does not match this release')
        with tarfile.open(source, 'r:gz') as archive:
            members = {entry.name: entry for entry in archive.getmembers()}
            if any(not entry.isfile() or '/' in entry.name or entry.name in ('.', '..') for entry in members.values()):
                raise ValueError('Unexpected server archive member')
            embedded = json.loads(archive.extractfile('manifest.json').read())
            if embedded != info:
                raise ValueError('Server archive differs from published metadata')
            for name, digest in info['files'].items():
                if name not in ('jam', 'jamd', 'SKILL.md') or hashlib.sha256(archive.extractfile(name).read()).hexdigest() != digest:
                    raise ValueError('Server archive checksum mismatch')
        protocols.add(info['protocol'])
        name = f'jam-v{version}-linux-{arch}.tar.gz'
        shutil.copyfile(source, output / name)
        assets.append({'name': name, 'platform': 'linux', 'arch': arch})
    if len(protocols) != 1:
        raise ValueError('Linux builds disagree on wire protocol')
    assets.insert(0, {'name': f'Jam-v{version}-macos-arm64.zip', 'platform': 'macos', 'arch': 'aarch64'})
    with zipfile.ZipFile(output / assets[0]['name']) as archive:
        marker = json.loads(archive.read('Jam.app/Contents/Resources/release.json'))
        if marker.get('version') != version or marker.get('platform') != 'macos' or marker.get('arch') != 'aarch64':
            raise ValueError('macOS bundle release marker does not match this release')
    for asset in assets:
        data = (output / asset['name']).read_bytes()
        if not data:
            raise ValueError(f'Empty release asset: {asset["name"]}')
        asset.update(sha256=hashlib.sha256(data).hexdigest(), size=len(data))
    manifest = {'schema': 1, 'version': version, 'protocol': protocols.pop(), 'min_macos': '14.0', 'assets': assets}
    path = output / 'manifest.json'
    path.write_text(json.dumps(manifest, sort_keys=True, indent=2) + '\n')
    public = subprocess.check_output(['openssl', 'pkey', '-in', str(key), '-pubout', '-outform', 'DER'])
    trusted = bytes.fromhex((ROOT / 'assets/update-public-key.hex').read_text().strip())
    if public != bytes.fromhex('302a300506032b6570032100') + trusted:
        raise ValueError('Release signing key does not match embedded updater trust key')
    signature = output / '.signature'
    try:
        subprocess.run(['openssl', 'pkeyutl', '-sign', '-rawin', '-inkey', str(key), '-in', str(path), '-out', str(signature)], check=True)
        data = signature.read_bytes()
        if len(data) != 64:
            raise ValueError('Invalid Ed25519 signature length')
        (output / 'manifest.sig').write_text(data.hex() + '\n')
    finally:
        signature.unlink(missing_ok=True)
    (output / 'SHA256SUMS').write_text(''.join(f'{a["sha256"]}  {a["name"]}\n' for a in assets))


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('version')
    parser.add_argument('--key', type=Path)
    args = parser.parse_args()
    if args.key:
        prepare(args.version.removeprefix('v'), args.key)
    else:
        secret = os.environ.get('JAM_RELEASE_SIGNING_KEY_PEM')
        if not secret:
            parser.error('Provide --key or JAM_RELEASE_SIGNING_KEY_PEM')
        with tempfile.TemporaryDirectory(prefix='jam-sign-') as temporary:
            key = Path(temporary) / 'key.pem'
            key.touch(mode=0o600)
            key.write_text(secret)
            prepare(args.version.removeprefix('v'), key)
