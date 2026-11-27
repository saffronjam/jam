"""Fetch an immutable Debian cloud image, checking its independently pinned digest."""

import argparse
import hashlib
import os
import tempfile
from pathlib import Path
from urllib.request import urlopen

IMAGE = "debian-12-genericcloud-arm64-20261117-2665.qcow2"
URL = "https://cloud.debian.org/images/cloud/bookworm/20261117-2665/" + IMAGE
SHA512 = (
    "32d67577a3d5631839edb684da1c3b28018c3660285a73368b047321cc530a49f"
    "205b042373ff8b5852f6a43df2342014b1dbb260b7cd1289e44a367e699b9b0"
)


def digest(path):
    result = hashlib.sha512()
    with path.open("rb") as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b""):
            result.update(block)
    return result.hexdigest()


def fetch(output):
    if output.exists():
        if digest(output) != SHA512:
            raise ValueError("Existing VM image differs from the pinned Debian image")
        return
    output.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.NamedTemporaryFile(
        prefix=output.name + ".", suffix=".partial", dir=output.parent, delete=False
    ) as stream:
        partial = Path(stream.name)
        try:
            with urlopen(URL, timeout=60) as response:
                for block in iter(lambda: response.read(1024 * 1024), b""):
                    stream.write(block)
            stream.flush()
            if digest(partial) != SHA512:
                raise ValueError("Downloaded Debian VM image failed SHA-512 verification")
            os.replace(partial, output)
        finally:
            partial.unlink(missing_ok=True)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("output", type=Path)
    fetch(parser.parse_args().output)
