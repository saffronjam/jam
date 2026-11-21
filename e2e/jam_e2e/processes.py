"""Read real shell identities without injecting terminal commands."""

import json
import re
import subprocess
import sys
from pathlib import Path

from .driver import wait


def pane_processes(profile):
    expected = {pane["id"] for pane in profile.layout()["panes"] if not pane["exited"]}
    assert expected, "No live panes to observe"
    receipt = profile.socket.parent / "release.json"

    def observe():
        if not receipt.is_file():
            return False
        daemon = json.loads(receipt.read_text())["pid"]
        rows = subprocess.run(
            ["/bin/ps", "-A", "-o", "pid=,ppid="],
            check=True,
            capture_output=True,
            text=True,
            timeout=5,
        ).stdout.splitlines()
        children = [
            int(pid) for row in rows for pid, parent in [row.split()] if int(parent) == daemon
        ]
        result = {}
        for pid in children:
            if sys.platform == "darwin":
                process = subprocess.run(
                    ["/bin/ps", "eww", "-p", str(pid), "-o", "command="],
                    check=False,
                    capture_output=True,
                    text=True,
                    timeout=5,
                )
                if process.returncode:
                    continue
                environment = process.stdout
                belongs = re.search(
                    r"(?:^|\s)JAM_SOCKET=" + re.escape(str(profile.socket)) + r"(?:\s|$)",
                    environment,
                )
                pane = re.search(r"(?:^|\s)JAM_PANE_ID=([^\s]+)(?:\s|$)", environment)
                pane_id = pane[1] if pane else None
            else:
                try:
                    entries = Path(f"/proc/{pid}/environ").read_bytes().split(b"\0")
                except FileNotFoundError:
                    continue
                environment = dict(entry.split(b"=", 1) for entry in entries if b"=" in entry)
                belongs = environment.get(b"JAM_SOCKET") == str(profile.socket).encode()
                pane_id = environment.get(b"JAM_PANE_ID", b"").decode()
            if belongs and pane_id in expected:
                assert pane_id not in result, "Multiple root processes claim the same pane"
                result[pane_id] = pid
        return result if result.keys() == expected else False

    result = wait(observe, "OS shell PIDs for every live pane")
    with (profile.root / "process.events").open("a") as log:
        log.write(json.dumps({"pane_pids": result}) + "\n")
    return result
