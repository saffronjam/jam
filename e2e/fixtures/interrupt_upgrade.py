"""Interrupt a real public-CLI upgrade before it mutates the private installation."""

import json
import os
import signal
import subprocess
import time
from pathlib import Path


def interrupt(root):
    result = subprocess.run(
        [os.environ["JAM_E2E_CLI"], "--json", "upgrade", "--yes"],
        check=True,
        capture_output=True,
        text=True,
        timeout=45,
    )
    reply = json.loads(result.stdout)
    assert reply.get("started") is True, reply
    journal = Path(reply["journal"]).resolve()
    assert journal.is_relative_to((root / "state/upgrades").resolve()), journal
    helper = journal.parent / "jam-updater"
    expected = f"{helper} __upgrade-apply {journal}"
    deadline = time.monotonic() + 5
    while True:
        rows = subprocess.run(
            ["/bin/ps", "-A", "-ww", "-o", "pid=,args="],
            check=True,
            capture_output=True,
            text=True,
            timeout=2,
        ).stdout.splitlines()
        matches = [
            int(pid) for row in rows for pid, command in [row.split(None, 1)] if command == expected
        ]
        assert len(matches) <= 1, "More than one private updater helper"
        if matches:
            pid = matches[0]
            break
        if time.monotonic() >= deadline:
            raise AssertionError("The real updater helper was not observed before interruption")
        time.sleep(0.005)
    os.kill(pid, signal.SIGSTOP)
    try:
        phase = json.loads(journal.read_text())["phase"]
        assert phase == "staged", f"Updater already advanced to {phase}; fixture is not ready"
    finally:
        # The observed helper belongs to this private transaction. Always reap
        # the injected interruption, including an unexpected-phase setup error.
        os.kill(pid, signal.SIGKILL)
    if phase == "staged":
        (root / "fixture-upgrade-interruption.log").write_text(
            json.dumps({"pid": pid, "journal": str(journal), "phase": phase, "signal": "SIGKILL"})
            + "\n"
        )
