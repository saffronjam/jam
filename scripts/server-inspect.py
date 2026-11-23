#!/usr/bin/env python3
"""Read-only identity of the current user's managed Linux Jam server."""
import json
import os
import pathlib
import socket
import struct
import subprocess


def inspect():
    base = pathlib.Path(os.environ.get('XDG_DATA_HOME', pathlib.Path.home() / '.local/share')) / 'jam/servers'
    path = pathlib.Path(os.environ.get('XDG_RUNTIME_DIR', '/run/user/' + str(os.getuid()))) / ('jam-' + str(os.getuid())) / 'jamd.sock'
    result = {'running': False, 'arch': os.uname().machine, 'identity': None, 'info': None, 'recovery': None}
    for receipt in sorted((base / 'transactions').glob('*.json')):
        value = json.loads(receipt.read_text())
        if value.get('phase') not in ('committed', 'rolled_back'):
            result['recovery'] = {'id': value['id'], 'phase': value['phase']}
            break
    with socket.socket(socket.AF_UNIX) as connection:
        connection.settimeout(3)
        try:
            connection.connect(str(path))
        except (FileNotFoundError, ConnectionRefusedError):
            return result
        result['running'] = True
        pid, uid, _ = struct.unpack('3i', connection.getsockopt(socket.SOL_SOCKET, socket.SO_PEERCRED, 12))
        if uid != os.getuid() or pid <= 1:
            raise RuntimeError('Server ownership could not be verified')
        exe = pathlib.Path('/proc/' + str(pid) + '/exe').resolve(strict=True)
        expected = (base / 'current/jamd').resolve(strict=True)
        if exe != expected:
            raise RuntimeError('The socket is not owned by the managed Jam server')
        info = json.loads(subprocess.check_output([str(exe), '--build-info'], text=True, timeout=5))
        unit = pathlib.Path.home() / '.config/systemd/user/jamd.service'
        import hashlib
        result['identity'] = {'pid': pid, 'start': pathlib.Path('/proc/' + str(pid) + '/stat').read_text().rsplit(')', 1)[1].split()[19],
                              'executable': str(exe), 'unit': hashlib.sha256(unit.read_bytes()).hexdigest()}
        result['info'] = info
        metadata = path.parent / 'release.json'
        if metadata.exists():
            running = json.loads(metadata.read_text())
            if running.get('pid') == pid:
                result['runtime_version'] = running.get('release_version')
        manifest = base / 'current/manifest.json'
        if manifest.exists():
            result['installed_version'] = json.loads(manifest.read_text()).get('release_version')
    return result


if __name__ == '__main__':
    print(json.dumps(inspect()))
