"""Install external provider protocol fixtures in the private run profile."""

import argparse
import json
import sys
from pathlib import Path


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("root", type=Path)
    parser.add_argument("--world", default="{}", help="External provider behavior as JSON")
    args = parser.parse_args()
    directory = args.root / "bin"
    if not directory.is_dir():
        raise SystemExit("Expected an already-created isolated profile/bin")
    (args.root / "home/fixture-world.json").write_text(json.dumps(json.loads(args.world)))
    python = directory / "python3"
    if not python.exists():
        python.symlink_to(sys.executable)
    source = Path(__file__).with_name("provider.py").read_text()
    interpreters = args.root / "libexec"
    interpreters.mkdir(exist_ok=True)
    for provider in ("claude", "codex"):
        interpreter = interpreters / provider
        if not interpreter.exists():
            interpreter.symlink_to(sys.executable)
        path = directory / provider
        # Keep the real process argv[0] provider-named, including on Linux.
        # Usage collectors inspect OS processes rather than Jam's board records.
        path.write_text("#!" + str(interpreter) + "\n" + source)
        path.chmod(0o700)


if __name__ == "__main__":
    main()
