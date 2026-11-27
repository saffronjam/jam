"""Observe actual PTY geometry and SIGWINCH, without application internals."""

import json
import select
import signal
import sys
import termios
import time
from pathlib import Path


def main(output, lifetime=30):
    assert 0 < lifetime <= 120, "Geometry observer lifetime must be bounded"
    observations = []

    def measure(*_):
        rows, columns = termios.tcgetwinsize(sys.stdin.fileno())
        observations.append({"rows": rows, "columns": columns})
        temporary = output.with_suffix(".tmp")
        temporary.write_text(json.dumps(observations))
        temporary.replace(output)

    signal.signal(signal.SIGWINCH, measure)
    measure()
    output.with_suffix(".ready").touch()
    deadline = time.monotonic() + lifetime
    while time.monotonic() < deadline:
        if select.select([sys.stdin], [], [], .2)[0]:
            if sys.stdin.readline().strip() == "exit":
                output.with_suffix(".stopped").touch()
                return
    raise AssertionError("Geometry probe was not stopped through its terminal")


if __name__ == "__main__":
    main(Path(sys.argv[1]), float(sys.argv[2]) if len(sys.argv) > 2 else 30)
