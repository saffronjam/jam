#!/usr/bin/env python3
"""Install the pinned Zig toolchain into the project cache, never globally."""
import hashlib, io, os, pathlib, platform, tarfile, urllib.request
root = pathlib.Path(__file__).resolve().parent.parent
os_name = {'Darwin': 'macos', 'Linux': 'linux'}[platform.system()]
arch = {'arm64': 'aarch64', 'aarch64': 'aarch64', 'x86_64': 'x86_64'}[platform.machine()]
hashes = {
 ('macos', 'x86_64'): '4f9a1c5269aa17ebda5e6d3c2b89d6cbf36f7d2b22a0306e9ab98f25f95529c6',
 ('macos', 'aarch64'): 'b607e9b9234790a008116ae5bdb71c6243b84b9fb42a53a9e70fde41c06c536a',
 ('linux', 'aarch64'): '9e8d11661d4ae3bd57702a3832781e23ad151dde5798e16a5ccd503f65234ff8',
 ('linux', 'x86_64'): '1cbe9df9f27e6b78d14ccbca43b6703a404ef79ef1c463de901d7f088d4e2026',
}
name = f'zig-{arch}-{os_name}-0.17.0'
cache = pathlib.Path(os.environ.get('JAM_BOOTSTRAP_CACHE', root / '.cache'))
cache.mkdir(parents=True, exist_ok=True)
directory = cache / name
if not (directory / 'zig').exists():
    data = urllib.request.urlopen(f'https://ziglang.org/download/0.17.0/{name}.tar.xz', timeout=120).read()
    if hashlib.sha256(data).hexdigest() != hashes[(os_name, arch)]:
        raise RuntimeError('Zig checksum mismatch')
    archive = tarfile.open(fileobj=io.BytesIO(data))
    for member in archive.getmembers():
        target = cache / member.name
        if pathlib.PurePosixPath(member.name).is_absolute() or '..' in pathlib.PurePosixPath(member.name).parts or not (member.isfile() or member.isdir()):
            raise RuntimeError('Unexpected archive member')
        if not target.resolve().is_relative_to(cache.resolve()):
            raise RuntimeError('Archive path escapes cache')
    archive.extractall(cache)
link = cache / 'zig'
if link.is_symlink():
    if link.resolve() != directory:
        link.unlink()
if not link.exists():
    link.symlink_to(name, target_is_directory=True)
print(f'Zig 0.17.0: {directory}')
