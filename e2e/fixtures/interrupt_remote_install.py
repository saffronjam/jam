#!/usr/bin/python3
"""Interrupt the owned remote installer at the real service-stop boundary."""

import json
import os
import signal
import sys
from pathlib import Path


def main():
    home = Path.home()
    arguments = sys.argv[1:]
    armed = home / "fixture-interrupt-install"
    if arguments != ["--user", "stop", "jamd.service"] or not armed.is_file():
        os.execv("/usr/bin/systemctl", ["systemctl", *arguments])

    # This file is installed only on the disposable SSH guest. Identify the
    # installer process and its durable receipt before interrupting it.
    assert home == Path("/home/fixture")
    pid = os.getppid()
    process = Path(f"/proc/{pid}")
    assert pid > 1 and process.stat().st_uid == os.getuid()
    exe = (process / "exe").resolve()
    assert exe.name.startswith("python3")
    argv = (process / "cmdline").read_bytes().rstrip(b"\0").split(b"\0")
    script = Path(os.fsdecode(argv[1]))
    assert script.name == "install.py" and script.is_file()
    assert script.parent.parent == Path("/tmp") and script.parent.name.startswith("tmp.")
    assert os.fsdecode(argv[2]) == str(script.parent)
    assert b"--restart-active" in argv
    directory = home / ".local/share/jam/servers/transactions"
    [receipt] = [value for path in directory.glob("*.json")
                 if (value := json.loads(path.read_text()))["phase"] == "stop_requested"]
    (home / "fixture-interrupted-install.json").write_text(json.dumps(receipt))
    armed.unlink()
    os.kill(pid, signal.SIGKILL)
    return 137


if __name__ == "__main__":
    sys.exit(main())
