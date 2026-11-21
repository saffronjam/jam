"""A real raw-mode PTY application; records OS input, not Jam controller calls."""

import json
import os
import select
import sys
import termios
import time
import tty
from pathlib import Path


def main(output, mode="raw"):
    fd = sys.stdin.fileno()
    previous = termios.tcgetattr(fd)
    data = bytearray()
    try:
        tty.setraw(fd)
        if mode == "keyboard":
            # Kitty keyboard protocol: disambiguation, event types, all keys.
            # https://sw.kovidgoyal.net/kitty/keyboard-protocol/
            os.write(sys.stdout.fileno(), b"\x1b[>11u\x1b[?u")
            response = bytearray()
            handshake = time.monotonic() + 3
            while b"\x1b[?11u" not in response:
                if time.monotonic() >= handshake:
                    raise AssertionError(
                        "Terminal did not confirm the requested keyboard event flags"
                    )
                if select.select([fd], [], [], 0.1)[0]:
                    response.extend(os.read(fd, 4096))
            # The Enter key that launched this process may still be releasing.
            # Only input after the ready file belongs to the tested gesture.
            while select.select([fd], [], [], 0.05)[0]:
                os.read(fd, 4096)
                if time.monotonic() >= handshake:
                    raise AssertionError("Terminal input did not settle before the test gesture")
        elif mode == "mouse":
            os.write(sys.stdout.fileno(), b"\x1b[?1000h\x1b[?1006h")
        elif mode != "raw":
            raise ValueError("Unknown PTY probe mode: " + mode)
        output.with_suffix(".ready").touch()
        deadline = time.monotonic() + 20
        while time.monotonic() < deadline:
            ready, _, _ = select.select([fd], [], [], 0.2)
            if ready:
                data.extend(os.read(fd, 4096))
                deadline = time.monotonic() + 0.5
    finally:
        if mode == "keyboard":
            os.write(sys.stdout.fileno(), b"\x1b[<u")
        elif mode == "mouse":
            os.write(sys.stdout.fileno(), b"\x1b[?1000l\x1b[?1006l")
        termios.tcsetattr(fd, termios.TCSANOW, previous)
    temporary = output.with_suffix(".tmp")
    temporary.write_text(json.dumps({"hex": data.hex()}))
    temporary.replace(output)


if __name__ == "__main__":
    main(Path(sys.argv[1]), sys.argv[2] if len(sys.argv) > 2 else "raw")
