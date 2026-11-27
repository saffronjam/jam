import fcntl
import threading
import base64
import hashlib
import uuid
import json
import os
from pathlib import Path
import platform
import runpy
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

INSTALLER = Path(__file__).resolve().parents[1] / 'remote-install.py'

class InstallTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.source = self.root / 'source'
        self.source.mkdir()
        self.calls = []
        self.build('a')
    def build(self, tag):
        files = {}
        for name in ['jam', 'jamd', 'SKILL.md']:
            data = (tag + name).encode()
            (self.source / name).write_bytes(data)
            files[name] = hashlib.sha256(data).hexdigest()
        (self.source / 'manifest.json').write_text(json.dumps({'release_version':'0.0.1', 'arch':platform.machine(), 'build':tag * 24, 'files':files}))
    def install(self, refuse=False, lingering=True, options=(), fail_restart=False, fail_operation=None, fail_rollback=False, on_run=None, response=None, main_pid=None):
        failed = False
        def run(args, **kwargs):
            nonlocal failed
            self.calls.append(args)
            if on_run is not None:
                on_run(args)
            if fail_rollback and 'restart' in args:
                raise subprocess.CalledProcessError(1, args)
            if ((fail_restart and 'restart' in args) or (fail_operation is not None and fail_operation in args)) and not failed:
                failed = True
                raise subprocess.CalledProcessError(1, args)
            if refuse and args[-1] == 'prepare-upgrade':
                raise subprocess.CalledProcessError(1, args)
        with patch.dict(os.environ, {'HOME':str(self.root), 'XDG_DATA_HOME':str(self.root / 'data'), 'XDG_RUNTIME_DIR':str(self.root / 'runtime')}), patch('sys.argv', [str(INSTALLER), str(self.source), *options]), patch('platform.system', return_value='Linux'), patch('subprocess.run', side_effect=run), patch('subprocess.check_output', side_effect=lambda args, **kwargs: str(main_pid) if main_pid is not None and 'MainPID' in args else json.dumps(response) if response is not None else ('yes\n' if lingering else 'no\n')):
            runpy.run_path(str(INSTALLER), run_name='__main__')
    def test_standalone_update_passes_live_panes_to_daemon_at_commit(self):
        self.install()
        self.build('b')
        state = self.root / '.local/state/jam'
        live, stopped = str(uuid.uuid4()), str(uuid.uuid4())
        (state / 'layout.json').write_text(json.dumps({'panes': [
            {'id': live, 'exited': False}, {'id': stopped, 'exited': True},
        ]}))
        committed = []
        def inspect(args):
            if 'commit-upgrade' in args:
                committed.append(json.loads((state / '.upgrade-boot.json').read_text()))
        self.install(options=['--restart-active'], on_run=inspect)
        self.assertEqual([hold['resume_panes'] for hold in committed], [[live]])

    def test_stopped_server_upgrade_preserves_stopped_state(self):
        self.install()
        self.build('b')
        self.calls.clear()
        self.install(options=['--restart-active', '--server-stopped'])
        self.assertEqual((self.root/'data/jam/servers/current').resolve().name,'b'*24)
        self.assertFalse(any('stop' in call or 'restart' in call or '--now' in call or 'prepare-upgrade' in call for call in self.calls))

    def test_stopped_mode_refuses_a_listening_daemon(self):
        import socket
        self.install()
        self.build('b')
        runtime=self.root/'runtime'/('jam-'+str(os.getuid()))
        runtime.mkdir(parents=True)
        with socket.socket(socket.AF_UNIX) as server:
            server.bind(str(runtime/'jamd.sock'))
            server.listen()
            self.calls.clear()
            with self.assertRaisesRegex(SystemExit,'Server is running'):
                self.install(options=['--restart-active','--server-stopped'])
        self.assertFalse(self.calls)
        self.assertEqual((self.root/'data/jam/servers/current').resolve().name,'a'*24)

    def approved_option(self):
        approval={'PrepareUpgradeApproved':{'token':str(uuid.uuid4()), 'incarnation':'old', 'build':'build', 'panes':['pane'], 'sessions':{'pane':'generation'}}}
        return approval, '--approved-state=' + base64.b64encode(json.dumps(approval).encode()).decode()

    def test_approval_without_session_generation_is_rejected_before_subprocesses(self):
        approval,_=self.approved_option()
        approval['PrepareUpgradeApproved']['sessions']={}
        option='--approved-state='+base64.b64encode(json.dumps(approval).encode()).decode()
        self.calls.clear()
        with self.assertRaisesRegex(SystemExit,'missing terminal session identities'):
            self.install(options=['--restart-active',option])
        self.assertFalse(self.calls)

    def test_approved_upgrade_uses_atomic_owned_stop(self):
        self.install()
        self.build('b')
        approval, option = self.approved_option()
        self.calls.clear()
        self.install(options=['--restart-active', option])
        prepare=next(call for call in self.calls if 'prepare-upgrade' in call)
        self.assertEqual(json.loads(prepare[-1]), approval)
        stop=next(call for call in self.calls if 'stop-upgrade' in call)
        self.assertEqual(stop[-1], approval['PrepareUpgradeApproved']['token'])
        self.assertFalse(any(call[-1]=='stop' for call in self.calls))

    def test_rejected_owned_stop_does_not_force_restart_unapproved_sessions(self):
        self.install()
        self.build('b')
        _, option=self.approved_option()
        self.calls.clear()
        with self.assertRaises(subprocess.CalledProcessError):
            self.install(options=['--restart-active', option], fail_operation='stop-upgrade')
        self.assertTrue(any('abort-upgrade' in call for call in self.calls))
        self.assertFalse(any('restart' in call for call in self.calls))
        self.assertEqual((self.root/'data/jam/servers/current').resolve().name, 'a'*24)

    def test_failed_new_daemon_restores_checkpoint_before_starting_old_version(self):
        self.install()
        state=self.root/'.local/state/jam'
        state.mkdir(parents=True, exist_ok=True)
        (state/'layout.json').write_text('old readable layout')
        (state/'coordination.sqlite').write_text('old database')
        self.build('b')
        def migrate(args):
            if 'restart' in args and (self.root/'data/jam/servers/current').resolve().name=='b'*24:
                (state/'layout.json').write_text('new unreadable layout')
                (state/'coordination.sqlite').write_text('new database')
                (state/'coordination.sqlite-wal').write_text('new wal')
        with self.assertRaises(subprocess.CalledProcessError):
            self.install(fail_restart=True, on_run=migrate)
        self.assertEqual((state/'layout.json').read_text(), 'old readable layout')
        self.assertEqual((state/'coordination.sqlite').read_text(), 'old database')
        self.assertFalse((state/'coordination.sqlite-wal').exists())

    def test_newer_installed_release_is_never_downgraded_even_with_force(self):
        self.install()
        current = self.root / 'data/jam/servers/current/manifest.json'
        old = json.loads(current.read_text())
        old['release_version'] = '0.0.10'
        current.write_text(json.dumps(old))
        self.build('b')
        count = len(self.calls)
        with self.assertRaisesRegex(SystemExit, 'Newer Jam server'):
            self.install(options=['--restart-active'])
        self.assertEqual(len(self.calls), count)
        self.assertEqual((self.root / 'data/jam/servers/current').resolve().name, 'a' * 24)

    def test_newer_activation_while_waiting_on_lock_is_not_downgraded(self):
        self.install()
        current = self.root / 'data/jam/servers/current/manifest.json'
        self.build('b')
        base = self.root / 'data/jam/servers'
        waiting = threading.Event()
        failures = []
        original_flock = fcntl.flock
        calls_before = len(self.calls)
        def wait_on_lock(file, operation):
            waiting.set()
            original_flock(file, operation)
        def install_candidate():
            try:
                self.install(options=['--restart-active'])
            except BaseException as error:
                failures.append(error)
        with (base / '.install.lock').open('w') as lock:
            original_flock(lock, fcntl.LOCK_EX)
            with patch('fcntl.flock', side_effect=wait_on_lock):
                worker = threading.Thread(target=install_candidate)
                worker.start()
                try:
                    self.assertTrue(waiting.wait(5), 'Installer did not reach install lock')
                    newer = json.loads(current.read_text())
                    newer['release_version'] = '0.0.10'
                    current.write_text(json.dumps(newer))
                finally:
                    original_flock(lock, fcntl.LOCK_UN)
                    worker.join(5)
                self.assertFalse(worker.is_alive(), 'Installer did not release the lock')
        self.assertEqual(len(failures), 1)
        self.assertIsInstance(failures[0], SystemExit)
        self.assertIn('Newer Jam server', str(failures[0]))
        self.assertEqual(calls_before, len(self.calls), 'Downgrade attempted to stop a server')
        self.assertEqual((base / 'current').resolve().name, 'a' * 24)

    def test_unversioned_requires_explicit_migration(self):
        self.install()
        current = self.root / 'data/jam/servers/current/manifest.json'
        old = json.loads(current.read_text())
        del old['release_version']
        current.write_text(json.dumps(old))
        self.build('b')
        with self.assertRaisesRegex(SystemExit, 'explicit migration'):
            self.install(options=['--restart-active'])
        self.install(options=['--restart-active', '--allow-unversioned'])

    def test_legacy_takeover_stops_verified_service_without_old_cli(self):
        self.install()
        self.build('b')
        identity = {'pid':42, 'start':'100', 'executable':'old', 'unit':'hash'}
        (self.source/'server-inspect.py').write_text('def inspect():\n    return '+repr({'identity':identity})+'\n')
        option = '--takeover-state=' + base64.b64encode(json.dumps({'takeover':identity}).encode()).decode()
        self.calls.clear()
        self.install(options=['--restart-active',option], main_pid=42)
        self.assertIn(['systemctl','--user','stop','jamd.service'], self.calls)
        self.assertFalse(any(call[-1]=='stop' for call in self.calls))
        self.assertEqual((self.root/'data/jam/servers/current').resolve().name, 'b'*24)

    def test_takeover_rejects_changed_server_before_stopping(self):
        self.install()
        self.build('b')
        (self.source/'server-inspect.py').write_text("def inspect():\n    return {'identity':{'pid':43}}\n")
        option = '--takeover-state=' + base64.b64encode(json.dumps({'takeover':{'pid':42}}).encode()).decode()
        self.calls.clear()
        with self.assertRaisesRegex(SystemExit, 'server changed'):
            self.install(options=['--restart-active',option], main_pid=43)
        self.assertFalse(self.calls)
        self.assertEqual((self.root/'data/jam/servers/current').resolve().name, 'a'*24)

    def test_first_install_and_idempotence(self):
        self.install()
        self.install()
        self.assertEqual((self.root / 'data/jam/servers/current').resolve().name, 'a' * 24)
        self.assertFalse(any(call[-1] == 'stop' for call in self.calls))
    def test_active_upgrade_never_stops_old_daemon(self):
        self.install()
        self.build('b')
        with self.assertRaises(subprocess.CalledProcessError): self.install(refuse=True)
        self.assertEqual((self.root / 'data/jam/servers/current').resolve().name, 'a' * 24)
        self.assertFalse(any(call[-1] == 'stop' for call in self.calls))
    def test_idle_upgrade_drains_before_stop(self):
        self.install()
        self.build('b')
        self.install()
        operations = [call[-1] for call in self.calls]
        self.assertLess(operations.index('prepare-upgrade'), operations.index('stop'))
        self.assertEqual((self.root / 'data/jam/servers/current').resolve().name, 'b' * 24)
        self.assertEqual(sum('--now' in call for call in self.calls), 1, 'Upgrade must not start then restart a daemon accepting new panes')
    def test_missing_lingering_does_not_fail_install_or_enable_it(self):
        self.install(lingering=False)
        self.assertTrue((self.root / 'data/jam/servers/current').exists())
        self.assertFalse(any('enable-linger' in call for call in self.calls))

    def test_corrupt_archive_does_not_install(self):
        (self.source / 'jamd').write_text('tampered')
        with self.assertRaises(SystemExit): self.install()
        self.assertFalse(self.calls)

    def test_deferred_upgrade_keeps_active_server(self):
        self.install()
        self.build('b')
        with self.assertRaises(SystemExit) as result:
            self.install(refuse=True, options=['--defer-if-busy'])
        self.assertEqual(result.exception.code, 75)
        self.assertEqual((self.root / 'data/jam/servers/current').resolve().name, 'a' * 24)
        self.assertFalse(any(call[-1] == 'stop' for call in self.calls))

    def test_confirmed_upgrade_can_restart_active_server(self):
        self.install()
        self.build('b')
        self.install(refuse=True, options=['--restart-active'])
        self.assertTrue(any(call[-1] == 'stop' for call in self.calls))
        self.assertEqual((self.root / 'data/jam/servers/current').resolve().name, 'b' * 24)

    def test_failed_health_check_restores_previous_executable(self):
        self.install()
        self.build('b')
        manifest = json.loads((self.source / 'manifest.json').read_text())
        manifest['server_build'] = 'expected-build'
        (self.source / 'manifest.json').write_text(json.dumps(manifest))
        with patch('time.monotonic', side_effect=[0, 11]), self.assertRaises(SystemExit):
            self.install()
        self.assertEqual((self.root / 'data/jam/servers/current').resolve().name, 'a' * 24)

    def test_corrupt_cached_build_is_not_executed(self):
        self.install()
        (self.root / ('data/jam/servers/' + 'a' * 24) / 'jamd').write_text('corrupt')
        count = len(self.calls)
        with self.assertRaises(SystemExit):
            self.install()
        self.assertEqual(count, len(self.calls))

    def test_unexpected_server_build_never_stops_the_daemon(self):
        self.install()
        self.build('b')
        count = len(self.calls)
        with self.assertRaises(SystemExit):
            self.install(options=['--restart-active', '--expected-build=different'])
        self.assertEqual(count, len(self.calls))
        self.assertEqual((self.root / 'data/jam/servers/current').resolve().name, 'a' * 24)

    def test_start_failure_restores_previous_executable(self):
        self.install()
        self.build('b')
        with self.assertRaises(subprocess.CalledProcessError):
            self.install(fail_restart=True)
        self.assertEqual((self.root / 'data/jam/servers/current').resolve().name, 'a' * 24)


    def test_reload_enable_and_stop_failures_restore_previous_server(self):
        self.install()
        self.build('b')
        for operation in ['daemon-reload', 'enable', 'stop']:
            with self.subTest(operation=operation):
                start = len(self.calls)
                with self.assertRaises(subprocess.CalledProcessError):
                    self.install(fail_operation=operation)
                self.assertEqual((self.root / 'data/jam/servers/current').resolve().name, 'a' * 24)
                self.assertIn(['systemctl', '--user', 'restart', 'jamd.service'], self.calls[start:])

    def test_service_write_failure_occurs_before_stopping_old_server(self):
        self.install()
        self.build('b')
        start = len(self.calls)
        original = Path.write_text
        def write(path, *args, **kwargs):
            if path.name.startswith('jamd.tmp-'):
                raise OSError('injected unit write failure')
            return original(path, *args, **kwargs)
        with patch.object(Path, 'write_text', write), self.assertRaises(OSError):
            self.install()
        self.assertEqual((self.root / 'data/jam/servers/current').resolve().name, 'a' * 24)
        self.assertEqual(self.calls[start:], [])

    def test_service_activation_write_failure_restarts_old_server(self):
        self.install()
        self.build('b')
        original = Path.replace
        def replace(path, target):
            if path.name.startswith('jamd.tmp-'):
                raise OSError('injected unit activation failure')
            return original(path, target)
        start = len(self.calls)
        with patch.object(Path, 'replace', replace), self.assertRaises(OSError):
            self.install()
        self.assertEqual((self.root / 'data/jam/servers/current').resolve().name, 'a' * 24)
        self.assertIn(['systemctl', '--user', 'restart', 'jamd.service'], self.calls[start:])

    def test_failed_first_install_removes_activation_and_new_unit(self):
        with self.assertRaises(subprocess.CalledProcessError):
            self.install(fail_operation='enable')
        self.assertFalse((self.root / 'data/jam/servers/current').exists())
        self.assertFalse((self.root / '.config/systemd/user/jamd.service').exists())
        self.assertIn(['systemctl', '--user', 'stop', 'jamd.service'], self.calls)
        self.assertIn(['systemctl', '--user', 'disable', 'jamd.service'], self.calls)

    def test_rollback_failure_reports_both_errors_and_retains_previous_build(self):
        self.install()
        self.build('b')
        with self.assertRaisesRegex(RuntimeError, 'Rollback also failed'):
            self.install(fail_operation='daemon-reload', fail_rollback=True)
        self.assertTrue((self.root / ('data/jam/servers/' + 'a' * 24)).is_dir())
        self.assertEqual((self.root / 'data/jam/servers/current').resolve().name, 'a' * 24)


    def latest_receipt(self):
        receipts = list((self.root / 'data/jam/servers/transactions').glob('*.json'))
        path = max(receipts, key=lambda p: p.stat().st_mtime_ns)
        return path, json.loads(path.read_text())

    def recover(self, receipt_id, extra=()):
        with self.assertRaises(SystemExit) as result:
            self.install(options=['--recover-transaction=' + receipt_id, '--server-stopped', *extra])
        self.assertEqual(result.exception.code, 0)

    def test_checkpoint_is_linked_to_journal_and_receipt_is_private(self):
        self.install()
        state = self.root / '.local/state/jam'
        (state / 'layout.json').write_text('old layout')
        journal = self.root / 'journal'
        journal.mkdir(mode=0o700)
        self.build('b')
        self.install(options=['--checkpoint-path=' + str(journal / 'checkpoint')])
        path, receipt = self.latest_receipt()
        self.assertEqual(receipt['checkpoint'], str(journal / 'checkpoint'))
        self.assertEqual(receipt['phase'], 'health_verified')
        self.assertEqual(receipt['old_build'], 'a' * 24)
        self.assertEqual(receipt['new_build'], 'b' * 24)
        self.assertEqual(path.stat().st_mode & 0o077, 0)
        self.assertEqual(json.loads((journal / 'checkpoint/complete.json').read_text()), True)
        self.assertEqual((journal / 'checkpoint/layout.json').read_text(), 'old layout')
        self.assertTrue((state / '.upgrade-boot.json').exists())
        self.assertFalse(any('commit-upgrade' in call for call in self.calls[-3:]))

    def test_hard_crash_after_activation_is_recoverable_without_reusing_consent(self):
        self.install()
        state = self.root / '.local/state/jam'
        (state / 'layout.json').write_text('old layout')
        self.build('b')
        program = r"""
import os, pathlib, platform, runpy, subprocess, sys
from unittest.mock import patch
root = pathlib.Path(sys.argv[2])
def run(args, **kwargs):
    if 'restart' in args:
        state = root / '.local/state/jam'
        assert (state / '.upgrade-boot.json').is_file()
        (state / 'layout.json').write_text('new migrated layout')
        os._exit(88)
with patch.dict(os.environ, {'HOME':str(root), 'XDG_DATA_HOME':str(root/'data'), 'XDG_RUNTIME_DIR':str(root/'runtime')}), patch('sys.argv', [sys.argv[1],str(root/'source')]), patch('platform.system',return_value='Linux'), patch('subprocess.run',side_effect=run), patch('subprocess.check_output',return_value='yes\n'):
    runpy.run_path(sys.argv[0],run_name='__main__')
"""
        result = subprocess.run([sys.executable, '-c', program, str(INSTALLER), str(self.root)], capture_output=True)
        self.assertEqual(result.returncode, 88, result.stderr.decode())
        path, receipt = self.latest_receipt()
        self.assertEqual(receipt['phase'], 'start_requested')
        self.assertEqual((self.root / 'data/jam/servers/current').resolve().name, 'b' * 24)
        with self.assertRaisesRegex(SystemExit, 'Unfinished'):
            self.install()
        self.calls.clear()
        self.recover(receipt['id'])
        self.assertEqual((self.root / 'data/jam/servers/current').resolve().name, 'a' * 24)
        self.assertEqual((state / 'layout.json').read_text(), 'old layout')
        self.assertFalse((state / '.upgrade-boot.json').exists())
        self.assertEqual(json.loads(path.read_text())['phase'], 'rolled_back')
        self.assertFalse(self.calls, 'Recovery must not replay a previous stop/restart consent')

    def test_failed_commit_retains_verified_new_state_and_recovers_forward(self):
        self.install()
        self.build('b')
        state = self.root / '.local/state/jam'
        def migrate(args):
            if 'restart' in args:
                (state / 'layout.json').write_text('verified new state')
        with self.assertRaisesRegex(RuntimeError, 'Verified server retained'):
            self.install(fail_operation='commit-upgrade', on_run=migrate)
        path, receipt = self.latest_receipt()
        self.assertEqual(receipt['phase'], 'commit_requested')
        self.assertEqual((self.root / 'data/jam/servers/current').resolve().name, 'b' * 24)
        self.recover(receipt['id'])
        self.assertEqual((self.root / 'data/jam/servers/current').resolve().name, 'b' * 24)
        self.assertEqual((state / 'layout.json').read_text(), 'verified new state')
        self.assertEqual(json.loads(path.read_text())['phase'], 'committed')
        self.assertFalse((state / '.upgrade-boot.json').exists())

    def test_recovery_rejects_unowned_style_receipt_and_corrupt_cached_build(self):
        self.install()
        self.build('b')
        with self.assertRaisesRegex(RuntimeError, 'Verified server retained'):
            self.install(fail_operation='commit-upgrade')
        path, receipt = self.latest_receipt()
        path.chmod(0o644)
        with self.assertRaisesRegex(SystemExit, 'Unsafe transaction'):
            self.install(options=['--recover-transaction=' + receipt['id'], '--server-stopped'])
        path.chmod(0o600)
        (self.root / ('data/jam/servers/' + 'a' * 24) / 'jam').write_text('corrupted previous executable')
        with self.assertRaisesRegex(SystemExit, 'checksum mismatch'):
            self.install(options=['--recover-transaction=' + receipt['id'], '--server-stopped'])
        self.assertEqual((self.root / 'data/jam/servers/current').resolve().name, 'b' * 24)

    def test_recovery_refuses_live_daemon_and_changed_activation(self):
        import socket
        self.install()
        self.build('b')
        with self.assertRaisesRegex(RuntimeError, 'Verified server retained'):
            self.install(fail_operation='commit-upgrade')
        _, receipt = self.latest_receipt()
        runtime = self.root / 'runtime' / ('jam-' + str(os.getuid()))
        runtime.mkdir(parents=True, exist_ok=True)
        with socket.socket(socket.AF_UNIX) as server:
            server.bind(str(runtime / 'jamd.sock'))
            server.listen()
            with self.assertRaisesRegex(SystemExit, 'fresh interruption confirmation'):
                self.install(options=['--recover-transaction=' + receipt['id'], '--server-stopped', '--restart-active'])
        current = self.root / 'data/jam/servers/current'
        current.unlink()
        other = self.root / ('data/jam/servers/' + 'c' * 24)
        other.mkdir(mode=0o700)
        current.symlink_to(other.name)
        with self.assertRaisesRegex(SystemExit, 'activation changed'):
            self.install(options=['--recover-transaction=' + receipt['id'], '--server-stopped'])


    def test_linked_receipt_finalizes_only_after_global_commit(self):
        self.install()
        self.build('b')
        token = str(uuid.uuid4())
        parent = self.root / 'journal'
        parent.mkdir(mode=0o700)
        self.install(options=['--checkpoint-path=' + str(parent / 'checkpoint'), '--transaction-token=' + token])
        path = self.root / ('data/jam/servers/transactions/' + token + '.json')
        self.assertEqual(json.loads(path.read_text())['phase'], 'health_verified')
        with self.assertRaisesRegex(SystemExit, 'boot gate remains held'):
            self.install(options=['--commit-transaction=' + token])
        (self.root / '.local/state/jam/.upgrade-boot.json').unlink()
        cached = self.root / 'data/jam/servers/current/manifest.json'
        value = json.loads(cached.read_text())
        value['server_build'] = 'fixture-build'
        cached.write_text(json.dumps(value))
        with self.assertRaisesRegex(SystemExit, 'Running server does not match'):
            self.install(options=['--commit-transaction=' + token], response={'HostInfo': {'build': 'other'}})
        with self.assertRaises(SystemExit) as completed:
            self.install(options=['--commit-transaction=' + token], response={'HostInfo': {'build': 'fixture-build'}})
        self.assertEqual(completed.exception.code, 0)
        self.assertEqual(json.loads(path.read_text())['phase'], 'committed')

    def test_stopped_local_receipt_has_deterministic_token_and_static_finalize(self):
        self.install()
        self.build('b')
        token = str(uuid.uuid4())
        self.install(options=['--transaction-token=' + token, '--server-stopped'])
        cached = self.root / 'data/jam/servers/current/manifest.json'
        value = json.loads(cached.read_text())
        value['server_build'] = 'fixture-build'
        cached.write_text(json.dumps(value))
        self.calls.clear()
        with self.assertRaises(SystemExit) as completed:
            self.install(options=['--commit-transaction=' + token, '--server-stopped'], response={'build': 'fixture-build'})
        self.assertEqual(completed.exception.code, 0)
        self.assertFalse(self.calls)
        self.assertFalse((self.root / '.local/state/jam/.upgrade-boot.json').exists())
        receipt = json.loads((self.root / ('data/jam/servers/transactions/' + token + '.json')).read_text())
        self.assertEqual(receipt['phase'], 'committed')

    def test_transaction_token_cannot_replace_approval_or_be_replayed(self):
        self.install()
        self.build('b')
        _, approval = self.approved_option()
        with self.assertRaisesRegex(SystemExit, 'does not match'):
            self.install(options=[approval, '--transaction-token=' + str(uuid.uuid4())])
        token = str(uuid.uuid4())
        self.install(options=['--transaction-token=' + token, '--server-stopped'])
        with self.assertRaisesRegex(SystemExit, 'already used'):
            self.install(options=['--transaction-token=' + token, '--server-stopped'])

if __name__ == '__main__': unittest.main()
