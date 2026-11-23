#!/usr/bin/env python3
"""Run inside the Linux build container. Produce a verified, self-contained archive."""
import hashlib, json, pathlib, platform, shutil, subprocess, tarfile
root = pathlib.Path(__file__).resolve().parent.parent
import os
os.environ['JAM_BOOTSTRAP_CACHE'] = str(root / '.cache' / ('remote-' + platform.machine()))
subprocess.run(['python3', 'scripts/bootstrap.py'], cwd=root, check=True)
import os
os.environ['PATH'] = str(pathlib.Path(os.environ['JAM_BOOTSTRAP_CACHE']) / 'zig') + ':' + os.environ['PATH']
subprocess.run(['cargo', 'build', '--locked', '--release', '-p', 'jam'], cwd=root, check=True)
arch = os.environ.get('CARGO_BUILD_TARGET', platform.machine()).split('-')[0]
if arch != platform.machine():
    loader = {'x86_64':'ld-linux-x86-64.so.2', 'aarch64':'ld-linux-aarch64.so.1'}[arch]
    prefix = pathlib.Path('/usr') / (arch + '-linux-gnu') / 'lib'
    runner = str(prefix / loader) + ' --library-path ' + str(prefix)
    os.environ['CARGO_TARGET_' + os.environ['CARGO_BUILD_TARGET'].upper().replace('-', '_') + '_RUNNER'] = runner
fixture = root / 'build/remote' / (arch + '.snapshot')
fixture.parent.mkdir(parents=True, exist_ok=True)
subprocess.run(['cargo', 'run', '--locked', '--release', '-p', 'jam-core', '--example', 'snapshot_fixture', '--', 'write', str(fixture)], cwd=root, check=True)
local = fixture.parent / 'local.snapshot'
if local.exists():
    subprocess.run(['cargo', 'run', '--locked', '--release', '-p', 'jam-core', '--example', 'snapshot_fixture', '--', 'verify', str(local)], cwd=root, check=True)
out = root / 'build/remote' / arch
out.mkdir(parents=True, exist_ok=True)
for name in ['jam', 'jamd']: shutil.copy2(pathlib.Path(os.environ.get('CARGO_TARGET_DIR', root / 'target')) / os.environ.get('CARGO_BUILD_TARGET', '') / 'release' / name, out / name)
shutil.copy2(root / 'skills/jam/SKILL.md', out / 'SKILL.md')
shutil.copy2(root / 'scripts/remote-install.py', out / 'install.py')
shutil.copy2(root / 'scripts/server-inspect.py', out / 'server-inspect.py')
subprocess.run(['python3', 'scripts/release/notices.py'], cwd=root, check=True)
shutil.copy2(root / 'build/DEPENDENCIES.txt', out / 'DEPENDENCIES.txt')
shutil.copy2(root / 'LICENSE', out / 'LICENSE')
files = {name: hashlib.sha256((out / name).read_bytes()).hexdigest() for name in ['jam','jamd','SKILL.md']}
build = hashlib.sha256(json.dumps(files, sort_keys=True).encode()).hexdigest()[:24]
runner = os.environ.get('CARGO_TARGET_' + os.environ.get('CARGO_BUILD_TARGET', '').upper().replace('-', '_') + '_RUNNER', '').split()
server_info = json.loads(subprocess.check_output(runner + [str(out / 'jamd'), '--build-info'], text=True))
manifest = {'release_version':server_info['release_version'], 'arch':arch, 'build':build, 'files': files, 'server_build':server_info['build'], 'protocol':server_info['protocol']}
(out / 'manifest.json').write_text(json.dumps(manifest))
(out.parent / (arch + '.json')).write_text(json.dumps(manifest))
with tarfile.open(out.parent / (arch + '.tar.gz'), 'w:gz') as archive:
    for path in out.iterdir(): archive.add(path, arcname=path.name)
