"""The size observer records real kernel PTY changes and SIGWINCH delivery."""

import fcntl
import json
import os
import pty
import subprocess
import sys
import termios
from pathlib import Path

from jam_e2e.driver import wait


def test_geometry_probe_observes_the_actual_pty_resize(tmp_path):
    master, slave = pty.openpty()
    termios.tcsetwinsize(slave, (24, 80))
    output = tmp_path / "geometry.json"

    def controlling_terminal():
        os.setsid()
        fcntl.ioctl(slave, termios.TIOCSCTTY, 0)
        os.tcsetpgrp(slave, os.getpgrp())

    child = subprocess.Popen(
        [sys.executable, str(Path(__file__).parents[1] / "jam_e2e/geometry_probe.py"), str(output)],
        stdin=slave, stdout=slave, stderr=slave, preexec_fn=controlling_terminal,
    )
    # The child owns the slave. Keeping another slave descriptor in the parent
    # can prevent Darwin's controlling-terminal teardown from completing.
    os.close(slave)
    try:
        wait(output.with_suffix(".ready").exists, "geometry observer startup")
        assert json.loads(output.read_text()) == [{"rows": 24, "columns": 80}]
        termios.tcsetwinsize(master, (36, 120))
        wait(lambda: json.loads(output.read_text())[-1] == {"rows": 36, "columns": 120},
             "kernel SIGWINCH observation")
        assert len(json.loads(output.read_text())) >= 2
        os.write(master, b"exit\n")
        wait(lambda: child.poll() is not None, "geometry observer exit", timeout=5)
        assert child.returncode == 0
        assert output.with_suffix(".stopped").is_file()
    finally:
        if child.poll() is None:
            child.kill()
            wait(lambda: child.poll() is not None, "geometry observer cleanup", timeout=5)
        os.close(master)
