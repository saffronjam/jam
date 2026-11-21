#!/usr/bin/env python3
"""Prepare an isolated fixture release version from an explicit source revision."""

import argparse
import io
import json
import re
import subprocess
import tarfile
import tomllib
from pathlib import Path


def prepare(source, output, version, cargo_target, revision="HEAD"):
    if not re.fullmatch(r"\d+\.\d+\.\d+", version):
        raise ValueError("Fixture version must be a stable numeric version")
    if output.exists():
        raise ValueError(f"Refusing to replace an existing variant: {output}")
    commit = subprocess.check_output(
        [
            "git",
            "-C",
            str(source),
            "rev-parse",
            "--verify",
            "--end-of-options",
            revision + "^{commit}",
        ],
        text=True,
    ).strip()
    archive = subprocess.check_output(
        ["git", "-C", str(source), "archive", "--format=tar", commit]
    )
    output.mkdir(parents=True)
    with tarfile.open(fileobj=io.BytesIO(archive)) as tree:
        tree.extractall(output, filter="data")
    manifest = output / "Cargo.toml"
    text = manifest.read_text()
    data = tomllib.loads(text)
    previous = data["workspace"]["package"]["version"]
    original = f'version = "{previous}"'
    if text.count(original) != 1:
        raise ValueError("Cannot uniquely locate the workspace release version")
    manifest.write_text(text.replace(original, f'version = "{version}"'))
    lock = output / "Cargo.lock"
    before = tomllib.loads(lock.read_text())["package"]
    subprocess.run(
        ["cargo", "update", "--workspace", "--offline"], cwd=output, check=True
    )
    after = tomllib.loads(lock.read_text())["package"]
    if [p for p in before if "source" in p] != [p for p in after if "source" in p]:
        raise ValueError(
            "Fixture version preparation changed a non-workspace dependency"
        )
    cargo_target.mkdir(parents=True, exist_ok=True)
    (output / "target").symlink_to(cargo_target.resolve(), target_is_directory=True)
    metadata = {
        "schema": 1,
        "source_commit": commit,
        "baseline_version": previous,
        "version": version,
    }
    (output / "fixture-variant.json").write_text(json.dumps(metadata, indent=2) + "\n")
    return metadata


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--version", required=True)
    parser.add_argument("--cargo-target", type=Path, required=True)
    parser.add_argument("--revision", default="HEAD")
    args = parser.parse_args()
    print(
        json.dumps(
            prepare(
                args.source.resolve(),
                args.output.resolve(),
                args.version,
                args.cargo_target.resolve(),
                args.revision,
            ),
            indent=2,
        )
    )


if __name__ == "__main__":
    main()
