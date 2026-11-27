"""Produce synchronized redraws and cursor states through a real terminal."""

import json
import sys
import time
from pathlib import Path


def main(path, mode):
    path = Path(path)

    def receipt(value):
        temp = path.with_suffix(".tmp")
        temp.write_text(json.dumps(value))
        temp.replace(path)

    def frame(rgb):
        color = ";".join(map(str, rgb))
        sys.stdout.write(f"\x1b[H\x1b[2J\x1b[48;2;{color}m" + (" " * 28 + "\r\n") * 4)
        sys.stdout.write("\x1b[0m\x1b[H")
        sys.stdout.flush()

    def send(value):
        sys.stdout.write(value)
        sys.stdout.flush()

    def requested(phase):
        deadline = time.monotonic() + 45
        while not path.with_suffix("." + phase).exists():
            if time.monotonic() >= deadline:
                raise TimeoutError("No render observer for " + phase)
            time.sleep(0.025)

    send("\x1b[?1049h\x1b[?25l")
    try:
        frame((24, 169, 78))
        receipt({"phase": "ready"})
        if mode == "cursor":
            for phase, control in (("steady", "\x1b[8;10H\x1b[2 q\x1b[?25h"),
                                   ("blink", "\x1b[1 q"), ("concealed", "\x1b[?25l")):
                requested(phase)
                send(control)
                receipt({"phase": phase})
            if sys.stdin.readline().strip() != "exit":
                raise ValueError("Expected exit from the native terminal")
            return
        requested("go")
        intervals = []
        for _ in range(5 if mode == "complete" else 1):
            start = time.monotonic_ns()
            # Split the escape sequence across two real PTY writes.
            send("\x1b[?20")
            time.sleep(0.01)
            send("26h")
            frame((236, 18, 174))
            receipt({"phase": "holding", "intervals": intervals})
            time.sleep(0.80)
            if mode == "complete":
                frame((28, 108, 232))
                send("\x1b[?2026l")
            end = time.monotonic_ns()
            intervals.append([start + 60_000_000, end - 20_000_000])
            receipt({"phase": "holding", "intervals": intervals})
            time.sleep(0.26)
        receipt({"phase": "finished", "intervals": intervals})
        if sys.stdin.readline().strip() != "exit":
            raise ValueError("Expected exit from the native terminal")
    finally:
        send("\x1b[?2026l\x1b[0 q\x1b[?25h\x1b[?1049l")
        path.with_suffix(".exited").touch()


if __name__ == "__main__":
    main(*sys.argv[1:])
