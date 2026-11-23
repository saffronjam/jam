"""Start the shipped daemon from a second installation under the owned profile."""

import os
from pathlib import Path
import shutil
import sys


def main(root, original):
    root = Path(root).resolve()
    assert Path(os.environ["JAM_STATE_DIR"]).resolve().is_relative_to(root)
    assert Path(os.environ["JAM_SOCKET"]).resolve().is_relative_to(root)
    other = root / "other-installation/Jam.app/Contents/MacOS/jamd"
    other.parent.mkdir(parents=True)
    shutil.copy2(original, other)
    os.execv(other, [str(other)])


if __name__ == "__main__":
    main(*sys.argv[1:])
