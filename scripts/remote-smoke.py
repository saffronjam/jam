#!/usr/bin/env python3
"""Create an isolated remote workspace, verify reattachment, then remove only test state."""
import json
import pathlib
import re
import shlex
import subprocess
import sys
import time

root = pathlib.Path(__file__).resolve().parent.parent
binary = str(root / 'target/debug/jam')
name = sys.argv[1]
remotes = json.loads(subprocess.check_output([binary, 'remote', 'list']))
remote = next(r for r in remotes if name in (r['id'], r['name']))
ssh = ['ssh', '-T', '-oBatchMode=yes', '-oStrictHostKeyChecking=yes', remote['destination']]
path = subprocess.check_output(ssh + ['mktemp -d /tmp/jam-smoke.XXXXXXXX'], text=True).strip()
workspace = None

def call(*args):
    return json.loads(subprocess.check_output([binary, '--json', '--remote', name, *args], text=True))

try:
    state = call('workspace', 'create', path, '--name', 'Jam remote smoke')['State']
    workspace = next(w['id'] for w in state['workspaces'] if w['path'] == path)
    pane = next(p['id'] for p in state['panes'] if p['workspace'] == workspace)
    call('pane', 'send', '--pane', pane, '--text', "printf '\\nJAM_SMOKE_PID:%s\\n' \"$$\"", '--enter')
    def pid():
        for _ in range(30):
            text = call('pane', 'capture', '--pane', pane)['Capture']
            found = re.search(r'^JAM_SMOKE_PID:(\d+)\s*$', text, re.M)
            if found: return found.group(1)
            time.sleep(0.1)
        raise RuntimeError('Remote shell did not respond')
    before = pid()
    # Each CLI process closes its own SSH connection on exit.
    time.sleep(1)
    assert pid() == before, 'Shell changed across disconnected clients'
    print('Remote workspace and reattachment passed. Shell PID ' + before)
finally:
    if workspace is not None:
        call('workspace', 'close', '--workspace', workspace)
    # Never recursively delete; retain unexpected files for inspection.
    subprocess.run(ssh + ['rmdir -- ' + shlex.quote(path)], check=True)
