"""Prove readiness and SIGINT receipts using an actual controlling PTY."""

import fcntl
import json
import os
import pty
import signal
import subprocess
import sys
import termios
from pathlib import Path

from jam_e2e.driver import wait


def test_interrupt_probe_receives_control_c_from_the_terminal(tmp_path):
    master, slave = pty.openpty()
    output = tmp_path / "interrupt.json"

    def controlling_terminal():
        os.setsid()
        fcntl.ioctl(slave, termios.TIOCSCTTY, 0)
        os.tcsetpgrp(slave, os.getpgrp())

    child = subprocess.Popen(
        [sys.executable, str(Path(__file__).parents[1] / "jam_e2e/interrupt_probe.py"), str(output)],
        stdin=slave, stdout=slave, stderr=slave, preexec_fn=controlling_terminal,
    )
    os.close(slave)
    try:
        wait(output.with_suffix(".ready").exists, "foreground job readiness")
        identity = json.loads(output.with_suffix(".ready").read_text())
        assert identity == {"pid": child.pid, "group": child.pid, "foreground": child.pid}
        assert not output.exists()
        os.write(master, b"\x03")
        wait(lambda: child.poll() is not None, "foreground job exits on terminal SIGINT")
        assert child.returncode == 0
        assert json.loads(output.read_text()) == {**identity, "signal": signal.SIGINT}
    finally:
        if child.poll() is None:
            child.kill()
            child.wait(timeout=5)
        os.close(master)
