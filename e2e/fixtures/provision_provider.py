"""Install external provider protocol fixtures in the private run profile."""

import argparse
import json
import shutil
import subprocess
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
    scripts = args.root / "libexec/provider-scripts"
    scripts.mkdir(parents=True, exist_ok=True)
    launcher = args.root / "libexec/provider-launcher"
    subprocess.run(
        ["cc", "-std=c11", "-O2", "-Wall", "-Wextra", "-Werror",
         "-DJAM_FIXTURE_PYTHON=" + json.dumps(sys.executable),
         "-DJAM_FIXTURE_SCRIPTS=" + json.dumps(str(scripts)),
         str(Path(__file__).with_name("provider_launcher.c")), "-o", str(launcher)],
        check=True, capture_output=True, timeout=20,
    )
    for provider in ("claude", "codex"):
        (scripts / provider).write_text(source)
        path = directory / provider
        # Darwin resolves a Python interpreter alias in process metadata. Keep
        # an actual native parent named like the real provider, with normal
        # stdio, exit status and signal forwarding to its protocol child.
        shutil.copy2(launcher, path)
        path.chmod(0o700)


if __name__ == "__main__":
    main()
