#!/usr/bin/env python3
"""Manage only the local Jam daemon. Start detaches, stop/restart end its panes."""
import argparse
import errno
import os
from pathlib import Path
import socket
import stat
import subprocess
import sys
import tempfile
import time

ROOT = Path(__file__).resolve().parent.parent
CLI = ROOT / 'target/debug/jam'


def socket_path():
    if 'JAM_SOCKET' in os.environ:
        return os.environ['JAM_SOCKET']
    runtime = Path(os.environ.get('XDG_RUNTIME_DIR', tempfile.gettempdir()))
    return str(runtime / f'jam-{os.geteuid()}' / 'jamd.sock')


def listening():
    with socket.socket(socket.AF_UNIX) as connection:
        connection.settimeout(0.5)
        try:
            connection.connect(socket_path())
            return True
        except OSError as error:
            if error.errno in (errno.ENOENT, errno.ECONNREFUSED):
                return False
            raise


def invoke(action):
    return subprocess.run([str(CLI), 'daemon', action], timeout=15)


def stop():
    if not listening():
        print('Local daemon is not running.')
        return 0
    print('Stopping local daemon and its panes.', flush=True)
    result = invoke('stop')
    if result.returncode:
        return result.returncode
    deadline = time.monotonic() + 10
    while listening():
        if time.monotonic() >= deadline:
            raise RuntimeError('Daemon did not stop. Refusing to start a second one')
        time.sleep(0.05)
    return 0


def start():
    if listening():
        result = invoke('info')
        if result.returncode == 0:
            print('Local daemon is already running.')
        return result.returncode
    default = Path.home() / ('Library/Application Support/Jam' if sys.platform == 'darwin' else '.local/state/jam')
    if sys.platform != 'darwin' and 'XDG_STATE_HOME' in os.environ:
        default = Path(os.environ['XDG_STATE_HOME']) / 'jam'
    state = Path(os.environ.get('JAM_STATE_DIR', default))
    state.mkdir(mode=0o700, parents=True, exist_ok=True)
    info = state.lstat()
    if not stat.S_ISDIR(info.st_mode) or info.st_uid != os.geteuid() or info.st_mode & 0o077:
        raise RuntimeError('Daemon state directory must be owned by you with mode 0700')
    log_path = state / 'jamd.log'
    fd = os.open(log_path, os.O_WRONLY | os.O_CREAT | os.O_APPEND | os.O_NOFOLLOW, 0o600)
    with os.fdopen(fd, 'ab') as log:
        process = subprocess.Popen([str(CLI), 'daemon', 'run'], stdin=subprocess.DEVNULL,
                                   stdout=log, stderr=log, start_new_session=True)
    deadline = time.monotonic() + 10
    while time.monotonic() < deadline:
        if listening():
            result = invoke('info')
            if result.returncode == 0:
                print(f'Local daemon started. Log: {log_path}')
                return 0
            break
        if process.poll() is not None:
            raise RuntimeError(f'Daemon exited ({process.returncode}). Inspect {log_path}')
        time.sleep(0.05)
    if process.poll() is None:
        process.terminate()
        try:
            process.wait(timeout=2)
        except subprocess.TimeoutExpired:
            process.kill()
            process.wait()
    raise RuntimeError(f'Daemon did not become ready. Inspect {log_path}')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('action', choices=['start', 'stop', 'restart', 'status'], default='status', nargs='?')
    action = parser.parse_args().action
    if action == 'status':
        if not listening():
            print('Local daemon is not running.')
            return 1
        return invoke('info').returncode
    if action in ('stop', 'restart'):
        result = stop()
        if result or action == 'stop':
            return result
    return start()


if __name__ == '__main__':
    try:
        sys.exit(main())
    except (OSError, RuntimeError, subprocess.TimeoutExpired) as error:
        print(f'Error: {error}', file=sys.stderr)
        sys.exit(1)
