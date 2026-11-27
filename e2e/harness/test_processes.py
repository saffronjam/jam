"""The observer identifies actual owned children and excludes another profile."""

import json
import os
import subprocess
import sys
from types import SimpleNamespace

from jam_e2e.processes import owned_process_snapshots, pane_processes, stop_process_tree


def test_failed_automation_reconnection_still_cleans_the_owned_native_app(tmp_path):
    from jam_e2e.driver import Desktop

    app = tmp_path / "Private.app"
    app.mkdir()
    child = subprocess.Popen([
        sys.executable, "-u", "-c",
        "import signal,sys,time; signal.signal(signal.SIGTERM,lambda *_:sys.exit(0)); "
        "print('ready',flush=True); time.sleep(30)",
    ], stdout=subprocess.PIPE, text=True)
    ui = Desktop("macos", "unused", app, SimpleNamespace(root=tmp_path))
    ui.external_app = ui.dialog_active = True
    ui.application_pids = lambda: {child.pid} if child.poll() is None else set()
    try:
        assert child.stdout.readline().strip() == "ready"
        assert ui.session is None  # Reconnecting the native runner failed.
        ui.quit()
        assert child.wait(timeout=5) == 0
        assert not ui.application_pids()
    finally:
        if child.poll() is None:
            child.kill()
            child.wait(timeout=5)
        child.stdout.close()


def test_actual_shell_pids_are_observed_without_writing_to_stdin(tmp_path):
    socket = tmp_path / "jam.sock"
    (tmp_path / "release.json").write_text(json.dumps({"pid": os.getpid()}))
    children = []
    try:
        for pane, owner in (
            ("first", socket),
            ("second", socket),
            ("first", tmp_path / "foreign.sock"),
        ):
            children.append(
                subprocess.Popen(
                    ["/bin/bash", "--noprofile", "--norc", "-c", "read -r line"],
                    env=dict(os.environ, JAM_SOCKET=str(owner), JAM_PANE_ID=pane),
                    stdin=subprocess.PIPE,
                )
            )
        profile = SimpleNamespace(
            root=tmp_path,
            socket=socket,
            layout=lambda: {
                "panes": [{"id": pane, "exited": False} for pane in ("first", "second")]
            },
        )
        assert pane_processes(profile) == {"first": children[0].pid, "second": children[1].pid}
        children[0].terminate()
        children[0].wait(timeout=5)
        children[0].stdin.close()
        children[0] = subprocess.Popen(
            ["/bin/bash", "--noprofile", "--norc", "-c", "read -r line"],
            env=dict(os.environ, JAM_SOCKET=str(socket), JAM_PANE_ID="first"),
            stdin=subprocess.PIPE,
        )
        assert pane_processes(profile) == {"first": children[0].pid, "second": children[1].pid}
    finally:
        for child in children:
            child.terminate()
            child.wait(timeout=5)
            child.stdin.close()


def test_diagnostics_keep_owned_descendants_and_exclude_unrelated_processes(tmp_path):
    from jam_e2e.driver import wait

    helper = tmp_path / "owned.py"
    helper.write_text(
        "import subprocess, sys\nfrom pathlib import Path\n"
        "child = subprocess.Popen([sys.executable, '-c', 'import time; time.sleep(30)'])\n"
        "try:\n Path(sys.argv[1]).write_text(str(child.pid))\n sys.stdin.read()\n"
        "finally:\n child.terminate()\n child.wait(timeout=5)\n"
    )
    ready = tmp_path / "child.pid"
    owned = subprocess.Popen([sys.executable, str(helper), str(ready)], stdin=subprocess.PIPE)
    foreign = subprocess.Popen([sys.executable, "-c", "import sys; sys.stdin.read()"], stdin=subprocess.PIPE)
    try:
        wait(ready.exists, "owned descendant startup")
        child = int(ready.read_text())
        profile = SimpleNamespace(root=tmp_path, socket=tmp_path / "socket")
        snapshot = owned_process_snapshots(profile)
        for rows in snapshot.values():
            pids = {int(row.split(None, 1)[0]) for row in rows}
            assert {owned.pid, child} <= pids
            assert foreign.pid not in pids
            assert os.getpid() not in pids
    finally:
        for process in (owned, foreign):
            process.stdin.close()
            process.wait(timeout=5)


def test_failed_client_cleanup_stops_transport_children_and_preserves_unrelated_process(tmp_path):
    from jam_e2e.driver import wait

    ready = tmp_path / "child.pid"
    script = (
        "import subprocess,sys,time\nfrom pathlib import Path\n"
        "child=subprocess.Popen([sys.executable,'-c','import time;time.sleep(30)'])\n"
        "Path(sys.argv[1]).write_text(str(child.pid))\ntime.sleep(30)\n"
    )
    parent = subprocess.Popen([sys.executable, "-c", script, str(ready)])
    foreign = subprocess.Popen([sys.executable, "-c", "import time;time.sleep(30)"])
    child = None
    try:
        wait(ready.exists, "actual transport child startup")
        child = int(ready.read_text())
        assert not stop_process_tree({parent.pid}, grace=0.5)
        parent.wait(timeout=5)
        state = subprocess.run(["/bin/ps", "-p", str(child), "-o", "stat="],
                               capture_output=True, text=True, timeout=5).stdout.strip()
        assert not state or state.startswith("Z"), "Owned child survived application cleanup"
        assert foreign.poll() is None, "Cleanup terminated an unrelated process"
    finally:
        stop_process_tree({parent.pid, foreign.pid, *([child] if child else [])}, grace=0.5)
        parent.wait(timeout=5)
        foreign.wait(timeout=5)
