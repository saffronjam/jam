"""The observer identifies actual owned children and excludes another profile."""

import json
import os
import subprocess
from types import SimpleNamespace

from jam_e2e.processes import pane_processes


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
