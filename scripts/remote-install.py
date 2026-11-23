#!/usr/bin/env python3
"""Install an immutable server build, draining only idle daemons."""
import base64
import fcntl
import getpass
import hashlib
import json
import os
import pathlib
import platform
import re
import shutil
import socket
import stat
import subprocess
import sys
import time
import uuid

os.environ.setdefault('XDG_RUNTIME_DIR', '/run/user/' + str(os.getuid()))
source = pathlib.Path(sys.argv[1])
options = set(sys.argv[2:])
takeover_values = [v.removeprefix('--takeover-state=') for v in options if v.startswith('--takeover-state=')]
options = {v for v in options if not v.startswith('--takeover-state=')}
if len(takeover_values) > 1:
    sys.exit('Multiple takeover approvals')
takeover = json.loads(base64.b64decode(takeover_values[0], validate=True))['takeover'] if takeover_values else None
approved_values = [value.removeprefix('--approved-state=') for value in options if value.startswith('--approved-state=')]
options = {value for value in options if not value.startswith('--approved-state=')}
if len(approved_values) > 1:
    sys.exit('Multiple upgrade approvals')
approval = None
if approved_values:
    approval = json.loads(base64.b64decode(approved_values[0], validate=True))
    fields = approval.get('PrepareUpgradeApproved')
    if not isinstance(fields, dict) or set(fields) != {'token', 'incarnation', 'build', 'panes', 'sessions'}:
        sys.exit('Invalid upgrade approval')
    uuid.UUID(fields['token'])
    if (not isinstance(fields['panes'], list) or not all(isinstance(pane, str) for pane in fields['panes'])
            or not isinstance(fields['sessions'], dict) or set(fields['sessions']) != set(fields['panes'])
            or not all(isinstance(generation, str) and generation for generation in fields['sessions'].values())):
        sys.exit('Upgrade approval is missing terminal session identities')
transaction_values = [value.removeprefix('--transaction-token=') for value in options if value.startswith('--transaction-token=')]
checkpoint_values = [value.removeprefix('--checkpoint-path=') for value in options if value.startswith('--checkpoint-path=')]
commit_values = [value.removeprefix('--commit-transaction=') for value in options if value.startswith('--commit-transaction=')]
recovery_values = [value.removeprefix('--recover-transaction=') for value in options if value.startswith('--recover-transaction=')]
options = {value for value in options if not value.startswith(('--checkpoint-path=', '--recover-transaction=', '--commit-transaction=', '--transaction-token='))}
if len(checkpoint_values) > 1 or len(recovery_values) > 1 or len(commit_values) > 1 or len(transaction_values) > 1 or (recovery_values and commit_values):
    sys.exit('Multiple transaction recovery/checkpoint arguments')
if recovery_values or commit_values:
    uuid.UUID((recovery_values or commit_values)[0])
if transaction_values:
    uuid.UUID(transaction_values[0])
    if approval is not None and transaction_values[0] != fields['token']:
        sys.exit('Transaction token does not match upgrade approval')
expected = [value.removeprefix('--expected-build=') for value in options if value.startswith('--expected-build=')]
options = {value for value in options if not value.startswith('--expected-build=')}
if len(expected) > 1:
    sys.exit('Multiple expected server builds')
if takeover is not None:
    options.add('--allow-unversioned')
if options - {'--restart-active', '--defer-if-busy', '--allow-unversioned', '--server-stopped'}:
    sys.exit('Unknown server install option')
force = '--restart-active' in options
stopped_mode = '--server-stopped' in options
manifest = json.loads((source / 'manifest.json').read_text())
if expected and manifest.get('server_build') != expected[0]:
    sys.exit('Server archive does not match the requested update')
if platform.system() != 'Linux' or platform.machine() != manifest['arch']:
    sys.exit('Unsupported server platform for this artifact')
if not re.fullmatch('[0-9a-f]{24}', manifest['build']):
    sys.exit('Invalid build identifier')
if set(manifest['files']) != {'jam', 'jamd', 'SKILL.md'}:
    sys.exit('Incomplete server artifact')
for name, digest in manifest['files'].items():
    if hashlib.sha256((source / name).read_bytes()).hexdigest() != digest:
        sys.exit('Invalid artifact checksum')
base = pathlib.Path(os.environ.get('XDG_DATA_HOME', pathlib.Path.home() / '.local/share')) / 'jam/servers'
base.mkdir(parents=True, exist_ok=True, mode=0o700)
unit = pathlib.Path.home() / '.config/systemd/user/jamd.service'
executable = str(base / 'current/jamd').replace('\\', '\\\\').replace('"', '\\"').replace('%', '%%')
content = '[Unit]\nDescription=Jam terminal server\n[Service]\nExecStart="' + executable + '"\nRestart=on-failure\nUMask=0077\n[Install]\nWantedBy=default.target\n'

CHECKPOINT_FILES = ('layout.json', 'layout.pre-migration.json', 'coordination.sqlite',
                    'coordination.sqlite-wal', 'coordination.sqlite-shm', 'app-settings.json',
                    '.upgrade-boot.json')
TERMINAL_PHASES = {'committed', 'rolled_back'}
VERIFIED_PHASES = {'health_verified', 'commit_requested', 'committed'}
state = pathlib.Path(os.environ.get('JAM_STATE_DIR',
    pathlib.Path(os.environ.get('XDG_STATE_HOME', pathlib.Path.home() / '.local/state')) / 'jam')).resolve()


def private_directory(path, create=True):
    if create:
        path.mkdir(parents=True, exist_ok=True, mode=0o700)
    info = path.lstat()
    if not stat.S_ISDIR(info.st_mode) or info.st_uid != os.getuid() or info.st_mode & 0o077:
        sys.exit('Transaction directory must be owned, private and not a symlink')


def owned_file(path, private=False):
    info = path.lstat()
    if not stat.S_ISREG(info.st_mode) or info.st_uid != os.getuid() or (private and info.st_mode & 0o077):
        sys.exit('Unsafe transaction file. Server left unchanged')


def sync_directory(path):
    fd = os.open(path, os.O_RDONLY | os.O_DIRECTORY)
    try:
        os.fsync(fd)
    finally:
        os.close(fd)


def atomic_json(path, value):
    temporary = path.with_name('.' + path.name + '.tmp-' + uuid.uuid4().hex)
    try:
        with open(temporary, 'x', opener=lambda path, flags: os.open(path, flags, 0o600)) as output:
            json.dump(value, output, sort_keys=True)
            output.write('\n')
            output.flush()
            os.fsync(output.fileno())
        temporary.replace(path)
        sync_directory(path.parent)
    finally:
        temporary.unlink(missing_ok=True)


def socket_is_absent():
    runtime_socket = pathlib.Path(os.environ['XDG_RUNTIME_DIR']) / ('jam-' + str(os.getuid())) / 'jamd.sock'
    with socket.socket(socket.AF_UNIX) as connection:
        connection.settimeout(1)
        try:
            connection.connect(str(runtime_socket))
        except (FileNotFoundError, ConnectionRefusedError):
            return
    sys.exit('Server is running. Obtain fresh interruption confirmation before recovery')


def validate_build(name):
    if not isinstance(name, str) or not re.fullmatch('[0-9a-f]{24}', name):
        sys.exit('Invalid transaction build identifier')
    build = base / name
    private_directory(build, create=False)
    owned_file(build / 'manifest.json')
    info = json.loads((build / 'manifest.json').read_text())
    if info.get('build') != name or set(info.get('files', {})) != {'jam', 'jamd', 'SKILL.md'}:
        sys.exit('Invalid cached transaction build manifest')
    for filename, digest in info['files'].items():
        owned_file(build / filename)
        if hashlib.sha256((build / filename).read_bytes()).hexdigest() != digest:
            sys.exit('Cached transaction build checksum mismatch')
    return build.resolve()


def restore_saved_checkpoint(path):
    if not (path / 'complete.json').exists():
        return False
    private_directory(path, create=False)
    owned_file(path / 'complete.json', private=True)
    owned_file(path / 'files.json', private=True)
    description = json.loads((path / 'files.json').read_text())
    if description.get('schema') != 1 or description.get('state') != str(state):
        sys.exit('Checkpoint belongs to another state profile')
    files = description.get('files', {})
    if set(files) - set(CHECKPOINT_FILES):
        sys.exit('Invalid checkpoint file list')
    for name, digest in files.items():
        owned_file(path / name)
        if hashlib.sha256((path / name).read_bytes()).hexdigest() != digest:
            sys.exit('State checkpoint checksum mismatch')
    private_directory(state)
    for name in CHECKPOINT_FILES:
        if name in files:
            target = state / name
            temporary = state / ('.' + name + '.restore-' + uuid.uuid4().hex)
            shutil.copy2(path / name, temporary)
            with temporary.open('rb') as stream:
                os.fsync(stream.fileno())
            temporary.replace(target)
        else:
            (state / name).unlink(missing_ok=True)
    sync_directory(state)
    return True


private_directory(base)
transactions = base / 'transactions'
private_directory(transactions)

with (base / '.install.lock').open('w') as lock:
    fcntl.flock(lock, fcntl.LOCK_EX)
    recovery_restart = False
    if takeover is not None and 'recovery_remote' in takeover:
        import runpy
        inspected = runpy.run_path(str(source / 'server-inspect.py'))['inspect']()
        identifier = takeover['recovery_remote']
        uuid.UUID(identifier)
        if inspected['identity'] != takeover['live'] or inspected['recovery'] != {'id': identifier, 'phase': takeover['phase']}:
            sys.exit('Recovery target changed. Inspect the server again')
        target = transactions / (identifier + '.json')
        owned_file(target, private=True)
        saved = json.loads(target.read_text())
        # A newer release cannot be replaced even during recovery.
        def checked_version(value):
            if value is None:
                return (0, 0, 0)
            if not re.fullmatch(r'(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)', value):
                sys.exit('Invalid release version. Server left unchanged')
            return tuple(map(int, value.split('.')))
        versions = [inspected.get('installed_version'), inspected.get('runtime_version'), (inspected.get('info') or {}).get('release_version')]
        if any(checked_version(v) > checked_version(manifest.get('release_version')) for v in versions):
            sys.exit('Newer Jam server is installed. Update Jam before recovery')
        if saved['phase'] in VERIFIED_PHASES:
            if (state / '.upgrade-boot.json').exists():
                subprocess.run([str(base / 'current/jam'), 'daemon', 'commit-upgrade', identifier], check=True, timeout=15)
            commit_values = [identifier]
        else:
            if inspected['running']:
                main_pid = subprocess.check_output(['systemctl','--user','show','jamd.service','-p','MainPID','--value'], text=True, timeout=5).strip()
                if main_pid != str(inspected['identity']['pid']):
                    sys.exit('Service ownership changed. Server left unchanged')
                subprocess.run(['systemctl','--user','stop','jamd.service'], check=True, timeout=15)
            socket_is_absent()
            stopped_mode = True
            recovery_values = [identifier]
            recovery_restart = saved.get('was_running', False)
        takeover = None
    if recovery_values or commit_values:
        if recovery_values and not stopped_mode:
            sys.exit('Transaction recovery requires --server-stopped and fresh consent')
        if recovery_values or stopped_mode:
            socket_is_absent()
        receipt_id = (recovery_values or commit_values)[0]
        receipt_path = transactions / (receipt_id + '.json')
        owned_file(receipt_path, private=True)
        receipt = json.loads(receipt_path.read_text())
        if receipt.get('schema') != 1 or receipt.get('id') != receipt_id or receipt.get('state') != str(state):
            sys.exit('Invalid transaction receipt or state profile')
        known_phases = TERMINAL_PHASES | VERIFIED_PHASES | {'prepared', 'stop_requested', 'stopped',
            'checkpoint_saved', 'activation_requested', 'activated', 'start_requested', 'started', 'rollback_failed'}
        if receipt.get('phase') not in known_phases:
            sys.exit('Unknown transaction recovery phase')
        new = validate_build(receipt.get('new_build'))
        old = validate_build(receipt['old_build']) if receipt.get('old_build') is not None else None
        current = base / 'current'
        active = current.resolve() if current.exists() else None
        if active not in (new, old):
            sys.exit('Server activation changed after this transaction. Refusing rollback')
        checkpoint = pathlib.Path(receipt['checkpoint'])
        if not checkpoint.is_absolute():
            sys.exit('Invalid checkpoint path')
        private_directory(checkpoint.parent)
        boot = state / '.upgrade-boot.json'
        if boot.exists():
            owned_file(boot, private=True)
            if json.loads(boot.read_text()).get('token') != receipt['id']:
                sys.exit('Boot gate belongs to another transaction. Refusing recovery')
        if commit_values:
            if receipt['phase'] not in VERIFIED_PHASES or active != new:
                sys.exit('Transaction is not verified or activation changed')
            if boot.exists():
                sys.exit('Transaction boot gate remains held. Verify and commit the new server first')
            inspect = [str(new / 'jamd'), '--build-info'] if stopped_mode else [str(new / 'jam'), '--json', 'daemon', 'info']
            info = json.loads(subprocess.check_output(inspect, text=True, timeout=10))
            expected_build = json.loads((new / 'manifest.json').read_text()).get('server_build')
            actual_build = info.get('build') if stopped_mode else info.get('HostInfo', {}).get('build')
            if not expected_build or actual_build != expected_build:
                sys.exit('Running server does not match this verified transaction')
            receipt['phase'] = 'committed'
        elif receipt['phase'] in VERIFIED_PHASES:
            if active != new:
                sys.exit('Verified server activation changed. Refusing recovery')
            (state / '.upgrade-boot.json').unlink(missing_ok=True)
            sync_directory(state)
            receipt['phase'] = 'committed'
        elif receipt['phase'] != 'rolled_back':
            if active == new and not (checkpoint / 'complete.json').exists() and old is not None:
                sys.exit('Activated transaction has no complete checkpoint. Refusing unsafe rollback')
            restore_saved_checkpoint(checkpoint)
            if old is not None:
                link = base / ('recover-' + uuid.uuid4().hex)
                link.symlink_to(old.name)
                link.replace(current)
            elif active == new:
                current.unlink()
            sync_directory(base)
            receipt['phase'] = 'rolled_back'
        atomic_json(receipt_path, receipt)
        if recovery_restart:
            subprocess.run(['systemctl', '--user', 'restart', 'jamd.service'], check=True, timeout=15)
        print('Recovered Jam server transaction ' + receipt['id'] + ': ' + receipt['phase'])
        sys.exit(0)
    for receipt_path in transactions.glob('*.json'):
        owned_file(receipt_path, private=True)
        pending = json.loads(receipt_path.read_text())
        if pending.get('phase') not in TERMINAL_PHASES:
            sys.exit('Unfinished Jam server upgrade transaction ' + pending.get('id', receipt_path.stem)
                     + '. Recover it with fresh consent before installing another build')
    if unit.exists() and unit.read_text() != content:
        sys.exit('Existing jamd service differs. Refusing to overwrite it')
    if takeover is not None:
        import runpy
        inspection = runpy.run_path(str(source / 'server-inspect.py'))['inspect']()
        if inspection['identity'] != takeover:
            sys.exit('The server changed. Inspect it again before taking over')
        managed_pid = subprocess.check_output(['systemctl', '--user', 'show', 'jamd.service', '-p', 'MainPID', '--value'], text=True, timeout=5).strip()
        if managed_pid != str(takeover['pid']):
            sys.exit('The service does not own this server. Server left unchanged')
    # Recheck under the cross-client install lock, before copying or stopping.
    def release(value):
        match = re.fullmatch(r'(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)', value or '')
        if not match:
            sys.exit('Missing or unsupported release version. Server left unchanged')
        return tuple(map(int, match.groups()))
    candidate = release(manifest.get('release_version'))
    previous_manifest = base / 'current/manifest.json'
    if previous_manifest.exists():
        installed = json.loads(previous_manifest.read_text()).get('release_version')
        if installed is None and '--allow-unversioned' not in options:
            sys.exit('Unversioned server requires explicit migration. Server left unchanged')
        if installed is not None and release(installed) > candidate:
            sys.exit('Newer Jam server is installed. Upgrade the client instead')
    metadata = pathlib.Path(os.environ['XDG_RUNTIME_DIR']) / ('jam-' + str(os.getuid())) / 'release.json'
    if metadata.exists():
        running = json.loads(metadata.read_text()).get('release_version')
        if running is None and takeover is not None:
            pass
        elif release(running) > candidate:
            sys.exit('Newer Jam server is running. Upgrade the client instead')
    if stopped_mode:
        runtime_socket = pathlib.Path(os.environ['XDG_RUNTIME_DIR']) / ('jam-' + str(os.getuid())) / 'jamd.sock'
        with socket.socket(socket.AF_UNIX) as connection:
            connection.settimeout(1)
            try:
                connection.connect(str(runtime_socket))
            except (FileNotFoundError, ConnectionRefusedError):
                pass
            else:
                sys.exit('Server is running. Explicit interruption confirmation is required')
    build = base / manifest['build']
    if not build.exists():
        temporary = base / (manifest['build'] + '.partial-' + uuid.uuid4().hex)
        temporary.mkdir(mode=0o700)
        for name in manifest['files']:
            shutil.copy2(source / name, temporary / name)
        shutil.copy2(source / 'manifest.json', temporary / 'manifest.json')
        temporary.rename(build)
    for name, digest in manifest['files'].items():
        if hashlib.sha256((build / name).read_bytes()).hexdigest() != digest:
            sys.exit('Cached server build checksum mismatch')
    current = base / 'current'
    old = current.resolve() if current.exists() else None
    upgrading = old is not None and (old != build.resolve() or force)
    # Stage all service-file writes before draining the running server. The
    # existing unit, if any, was checked above and already has this content.
    unit_existed = unit.exists()
    unit.parent.mkdir(parents=True, exist_ok=True)
    temporary_unit = unit.with_suffix('.tmp-' + uuid.uuid4().hex)
    stopped_old = False
    activated = False
    unit_replaced = False
    start_attempted = False
    admission_acquired = False
    transaction_id = transaction_values[0] if transaction_values else (fields['token'] if approval is not None else str(uuid.uuid4()))
    receipt_path = transactions / (transaction_id + '.json')
    if receipt_path.exists():
        sys.exit('Transaction identifier was already used. Recover rather than replaying it')
    if checkpoint_values:
        checkpoint = pathlib.Path(checkpoint_values[0])
        if not checkpoint.is_absolute():
            sys.exit('Checkpoint path must be absolute')
        private_directory(checkpoint.parent)
    else:
        checkpoint = base / ('checkpoint-' + transaction_id)
    receipt = {'schema': 1, 'id': transaction_id, 'old_build': old.name if old else None,
               'new_build': build.name, 'checkpoint': str(checkpoint), 'state': str(state),
               'phase': 'prepared', 'was_running': not stopped_mode, 'unit_existed': unit_existed}

    def phase(value):
        receipt['phase'] = value
        atomic_json(receipt_path, receipt)

    def save_checkpoint():
        checkpoint.mkdir(mode=0o700)
        hashes = {}
        for name in CHECKPOINT_FILES:
            if (state / name).exists():
                owned_file(state / name)
                target = checkpoint / name
                shutil.copy2(state / name, target)
                with target.open('rb') as stream:
                    os.fsync(stream.fileno())
                hashes[name] = hashlib.sha256(target.read_bytes()).hexdigest()
        atomic_json(checkpoint / 'files.json', {'schema': 1, 'state': str(state), 'files': hashes})
        atomic_json(checkpoint / 'complete.json', True)
        phase('checkpoint_saved')

    def restore_checkpoint():
        return restore_saved_checkpoint(checkpoint)

    phase('prepared')

    def restore_previous():
        if old is not None:
            if activated and start_attempted:
                subprocess.run(['systemctl', '--user', 'stop', 'jamd.service'], check=True)
            if activated:
                previous = base / ('rollback-' + uuid.uuid4().hex)
                previous.symlink_to(old.name)
                previous.replace(current)
            if stopped_old:
                restore_checkpoint()
                subprocess.run(['systemctl', '--user', 'restart', 'jamd.service'], check=True)
            elif admission_acquired:
                subprocess.run([str(old / 'jam'), 'daemon', 'abort-upgrade', fields['token']], check=True)
        elif activated:
            # A failed first install has no previous executable. Stop any new
            # instance before removing its activation; retain the cached build.
            if start_attempted:
                subprocess.run(['systemctl', '--user', 'stop', 'jamd.service'], check=True)
            restore_checkpoint()
            current.unlink()
            if not unit_existed and unit_replaced:
                if start_attempted:
                    subprocess.run(['systemctl', '--user', 'disable', 'jamd.service'], check=True)
                unit.unlink()
                subprocess.run(['systemctl', '--user', 'daemon-reload'], check=True)

    try:
        temporary_unit.write_text(content)
        if upgrading and stopped_mode:
            save_checkpoint()
        elif upgrading:
            if approval is not None:
                subprocess.run([str(old / 'jam'), 'daemon', 'prepare-upgrade',
                                '--approved-json', json.dumps(approval)], check=True)
                admission_acquired = True
            elif not force:
                try:
                    subprocess.run([str(old / 'jam'), 'daemon', 'prepare-upgrade'], check=True)
                except subprocess.CalledProcessError:
                    if '--defer-if-busy' in options:
                        sys.exit(75)
                    raise
            # A failed stop can mean that the server stopped but its response
            # was lost. Recovery must attempt to start the previous executable.
            phase('stop_requested')
            if takeover is not None:
                if runpy.run_path(str(source / 'server-inspect.py'))['inspect']()['identity'] != takeover:
                    raise RuntimeError('The server changed before shutdown. Inspect it again')
                stopped_old = True
                subprocess.run(['systemctl', '--user', 'stop', 'jamd.service'], check=True, timeout=15)
                socket_is_absent()
            elif admission_acquired:
                # A rejected/expired token must not trigger a forceful restart
                # of a server which may now have unapproved sessions.
                subprocess.run([str(old / 'jam'), 'daemon', 'stop-upgrade', fields['token']], check=True)
                stopped_old = True
            else:
                stopped_old = True
                subprocess.run([str(old / 'jam'), 'daemon', 'stop'], check=True)
            phase('stopped')
            save_checkpoint()
        if old is None:
            save_checkpoint()
        if (old is None or upgrading) and not stopped_mode:
            private_directory(state)
            atomic_json(state / '.upgrade-boot.json', {'token': transaction_id})
        if old is None or upgrading:
            phase('activation_requested')
            next_link = base / ('next-' + uuid.uuid4().hex)
            next_link.symlink_to(build.name)
            next_link.replace(current)
            activated = True
            sync_directory(base)
            phase('activated')
        temporary_unit.replace(unit)
        unit_replaced = True
        subprocess.run(['systemctl', '--user', 'daemon-reload'], check=True)
        enable = ['systemctl', '--user', 'enable']
        if not upgrading and not stopped_mode:
            enable.append('--now')
        phase('start_requested')
        start_attempted = not stopped_mode
        subprocess.run(enable + ['jamd.service'], check=True)
        if upgrading and not stopped_mode:
            subprocess.run(['systemctl', '--user', 'restart', 'jamd.service'], check=True)
        phase('started' if not stopped_mode else 'activated')
        if manifest.get('server_build') and stopped_mode:
            info = json.loads(subprocess.check_output([str(build / 'jamd'), '--build-info'], text=True, timeout=10))
            if info.get('build') != manifest['server_build']:
                sys.exit('Installed server binary failed its version check')
        if manifest.get('server_build') and not stopped_mode:
            deadline = time.monotonic() + 10
            while True:
                remaining = deadline - time.monotonic()
                if remaining <= 0:
                    sys.exit('Updated server failed its health check')
                try:
                    info = json.loads(subprocess.check_output(
                        [str(build / 'jam'), '--json', 'daemon', 'info'],
                        text=True, timeout=remaining))
                    if info['HostInfo']['build'] == manifest['server_build']:
                        break
                except (subprocess.CalledProcessError, subprocess.TimeoutExpired,
                        json.JSONDecodeError, KeyError, TypeError):
                    pass
                time.sleep(min(0.1, max(0, deadline - time.monotonic())))
        phase('health_verified')
        if (old is None or upgrading) and not stopped_mode and not checkpoint_values:
            phase('commit_requested')
            subprocess.run([str(build / 'jam'), 'daemon', 'commit-upgrade', transaction_id], check=True)
            (state / '.upgrade-boot.json').unlink(missing_ok=True)
            sync_directory(state)
        if stopped_mode or not checkpoint_values:
            phase('committed')
    except BaseException as error:
        # After verified health, commit may have resumed work even when its
        # response was lost. Never roll that work back on uncertain delivery.
        boot = state / '.upgrade-boot.json'
        if boot.exists():
            owned_file(boot, private=True)
            if json.loads(boot.read_text()).get('token') != receipt['id']:
                sys.exit('Boot gate belongs to another transaction. Refusing recovery')
        if receipt['phase'] in VERIFIED_PHASES:
            raise RuntimeError('Verified server retained. Finish transaction recovery with fresh consent') from error
        try:
            restore_previous()
            phase('rolled_back')
        except BaseException as rollback_error:
            phase('rollback_failed')
            raise RuntimeError(
                f'Server installation failed: {error}\nRollback also failed: {rollback_error}\n'
                'The previous build is retained. Inspect the user service before retrying.'
            ) from error
        raise
    finally:
        temporary_unit.unlink(missing_ok=True)
    # Persistence is advisory after a successful health check. Failure to query
    # loginctl must not turn a completed installation into a failed transaction.
    try:
        linger = subprocess.check_output(
            ['loginctl', 'show-user', str(os.getuid()), '-p', 'Linger', '--value'],
            text=True, timeout=10).strip()
    except (subprocess.CalledProcessError, subprocess.TimeoutExpired, OSError):
        print('Server installed. Persistence could not be checked. Inspect loginctl show-user.')
    else:
        if linger != 'yes':
            print('Server installed. Persistence needs: loginctl enable-linger ' + getpass.getuser())
