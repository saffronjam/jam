#!/usr/bin/env python3
"""Collect original dependency notices. Fail closed on missing provenance."""
import hashlib
import json
import os
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[2]
PATTERNS = ('LICENSE*', 'LICENCE*', 'COPYING*', 'NOTICE*', 'license*', 'licenses/*')


def original_texts(directory):
    return sorted({path for pattern in PATTERNS for path in directory.glob(pattern) if path.is_file()})


def collect():
    metadata = json.loads(subprocess.check_output(['cargo', 'metadata', '--locked', '--format-version', '1'], cwd=ROOT))
    # Cargo metadata lists test-only and foreign-platform packages too. Follow
    # only normal/build edges reachable from the actual shipped entry points.
    shipped = set()
    for target, roots in (
        ('aarch64-apple-darwin', ('jam', 'jam-native')),
        ('x86_64-unknown-linux-gnu', ('jam',)),
        ('aarch64-unknown-linux-gnu', ('jam',)),
    ):
        graph = json.loads(subprocess.check_output([
            'cargo', 'metadata', '--locked', '--format-version', '1',
            '--filter-platform', target], cwd=ROOT))
        nodes = {node['id']: node for node in graph['resolve']['nodes']}
        todo = [p['id'] for p in graph['packages'] if p['source'] is None and p['name'] in roots]
        reached = set()
        while todo:
            package_id = todo.pop()
            if package_id in reached:
                continue
            reached.add(package_id)
            for dep in nodes[package_id]['deps']:
                if any(kind['kind'] != 'dev' for kind in dep['dep_kinds']):
                    todo.append(dep['pkg'])
        shipped.update(reached)
    index_path = ROOT / 'assets/third-party/index.json'
    index = json.loads(index_path.read_text()) if index_path.exists() else {}
    sections = ['# Jam third-party notices\n\nOriginal projects retain their respective licenses.\n']
    missing = []
    for package in sorted(metadata['packages'], key=lambda p: (p['name'], p['version'])):
        if package['source'] is None or package['id'] not in shipped:
            continue
        directory = Path(package['manifest_path']).parent
        candidates = original_texts(directory)
        if package.get('license_file'):
            candidates.append(directory / package['license_file'])
        texts = [f'\n## {path.name}\n\n' + path.read_text(errors='strict') for path in sorted(set(candidates)) if path.is_file()]
        if not texts:
            for entry in index.get(package['name'] + '@' + package['version'], []):
                path = ROOT / 'assets/third-party' / entry['file']
                if hashlib.sha256(path.read_bytes()).hexdigest() != entry['sha256']:
                    raise ValueError(f'Vendored license checksum mismatch: {path}')
                texts.append(f'\n## {entry["source"]}\n\n' + path.read_text())
        if package['name'] == 'libsqlite3-sys':
            sqlite = directory / 'sqlite3/sqlite3.c'
            if not sqlite.exists():
                missing.append('Bundled SQLite public-domain notice')
            else:
                texts.append('\n## Bundled SQLite notice\n\n' + '\n'.join(sqlite.read_text().splitlines()[:21]))
        if not texts:
            missing.append(f"{package['name']} {package['version']} ({package.get('license')})")
        sections.append(f"\n# {package['name']} {package['version']}\n\nLicense: {package.get('license') or 'see included text'}\n" + '\n'.join(texts))
    # The native terminal library and its Zig dependencies are not Cargo
    # packages. Collect their actual vendored texts from the pinned build.
    target = Path(metadata['target_directory'])
    ghostty = sorted(target.glob('**/build/libghostty-vt-sys-*/out/ghostty-src'))
    if not ghostty:
        missing.append('Ghostty VT built source (build jam before collecting notices)')
    else:
        directory = ghostty[-1]
        candidates = [directory / 'LICENSE']
        for dep in sorted((directory / 'zig-pkg').iterdir()):
            if dep.is_dir():
                texts = original_texts(dep)
                if not texts:
                    missing.append(f'Ghostty Zig dependency {dep.name}')
                candidates.extend(texts)
        for path in candidates:
            sections.append(f'\n# Ghostty VT / {path.relative_to(directory)}\n\n' + path.read_text())
    zig = Path(os.environ.get('JAM_BOOTSTRAP_CACHE', ROOT / '.cache')) / 'zig/LICENSE'
    if not zig.exists():
        missing.append('Zig toolchain LICENSE')
    else:
        sections.append('\n# Zig toolchain\n\n' + zig.read_text())
    # Statically linked Rust runtime code is outside Cargo's package graph.
    sysroot = Path(subprocess.check_output(['rustc', '--print', 'sysroot'], text=True).strip())
    runtime = sysroot / 'share/doc/rust'
    runtime_files = [runtime / 'COPYRIGHT-library.html'] + [runtime / 'licenses' / name for name in ('MIT.txt', 'Apache-2.0.txt', 'LLVM-exception.txt')]
    for path in runtime_files:
        if not path.is_file():
            missing.append('Rust runtime notice: ' + str(path))
        else:
            sections.append('\n# Rust runtime / ' + str(path.relative_to(runtime)) + '\n\n' + path.read_text())
    for name in ('fonts/OFL.txt', 'agent-icons/LICENSE'):
        path = ROOT / 'assets' / name
        if not path.exists():
            missing.append(str(path))
        else:
            sections.append(f'\n# {name}\n\n' + path.read_text())
    if missing:
        raise SystemExit('Missing original license texts (resolve before release):\n' + '\n'.join(missing))
    output = ROOT / 'build/DEPENDENCIES.txt'
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text('\n'.join(sections))
    print(f'Collected notices for {len(shipped)} Cargo packages and native dependencies')


if __name__ == '__main__':
    collect()
