"""Record a real foreground job receiving the terminal's SIGINT."""

import json
import os
import signal
import sys
import time
from pathlib import Path


def main(output):
    identity = {"pid": os.getpid(), "group": os.getpgrp(), "foreground": os.tcgetpgrp(0)}
    assert identity["group"] == identity["foreground"], "Probe must own the terminal foreground"

    def interrupted(number, _frame):
        output.write_text(json.dumps({**identity, "signal": number}))
        raise SystemExit(0)

    signal.signal(signal.SIGINT, interrupted)
    output.with_suffix(".ready").write_text(json.dumps(identity))
    time.sleep(60)
    raise AssertionError("The foreground job never received Control-C")


if __name__ == "__main__":
    main(Path(sys.argv[1]))
