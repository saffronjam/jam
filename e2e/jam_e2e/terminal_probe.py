"""A real raw-mode PTY application; records OS input, not Jam controller calls."""

import json
import os
import select
import sys
import termios
import time
import tty
from pathlib import Path


def main(output):
    fd = sys.stdin.fileno()
    previous = termios.tcgetattr(fd)
    data = bytearray()
    try:
        tty.setraw(fd)
        output.with_suffix(".ready").touch()
        deadline = time.monotonic() + 20
        while time.monotonic() < deadline:
            ready, _, _ = select.select([fd], [], [], 0.2)
            if ready:
                data.extend(os.read(fd, 4096))
                deadline = time.monotonic() + 0.5
    finally:
        termios.tcsetattr(fd, termios.TCSANOW, previous)
    temporary = output.with_suffix(".tmp")
    temporary.write_text(json.dumps({"hex": data.hex()}))
    temporary.replace(output)


if __name__ == "__main__":
    main(Path(sys.argv[1]))
