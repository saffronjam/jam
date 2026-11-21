"""Sign complete fixture releases containing real macOS and Linux installation assets."""

import argparse
import json
import subprocess
from pathlib import Path

from services import load_config, signed_manifest


def sign(artifacts, trust, versions, daemon):
    info = json.loads(subprocess.check_output([str(daemon.resolve()), "--build-info"], text=True))
    config = load_config(trust / "services.json")
    for version in versions:
        directory = artifacts / ("v" + version)
        assets = []
        for platform, arch, name in (
            ("macos", "aarch64", f"Jam-v{version}-macos-arm64.zip"),
            ("linux", "aarch64", f"jam-v{version}-linux-aarch64.tar.gz"),
            ("linux", "x86_64", f"jam-v{version}-linux-x86_64.tar.gz"),
        ):
            path = directory / name
            if not path.is_file():
                raise ValueError("Missing real fixture installation asset: " + str(path))
            assets.append({"path": str(path), "name": name, "platform": platform, "arch": arch})
        signed_manifest(directory, config, version, info["protocol"], assets)
    (artifacts / "fixture-releases.json").write_text(
        json.dumps({"schema": 1, "versions": versions, "protocol": info["protocol"]}, indent=2)
        + "\n"
    )


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--artifacts", type=Path, required=True)
    parser.add_argument("--trust", type=Path, required=True)
    parser.add_argument("--versions", nargs=2, required=True)
    parser.add_argument("--daemon", type=Path, required=True)
    args = parser.parse_args()
    sign(args.artifacts, args.trust, args.versions, args.daemon)


if __name__ == "__main__":
    main()
