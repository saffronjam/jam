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


def prepare(source, output, version, cargo_target, revision="HEAD", wire_protocol=None,
            failing_daemon=False, limited_capabilities=False):
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
    protocol_file = output / "crates/jam-core/src/protocol.rs"
    protocol_source = protocol_file.read_text()
    declaration = r'pub const VERSION: &str = "([^"]+)";'
    matches = list(re.finditer(declaration, protocol_source))
    if len(matches) != 1:
        raise ValueError("Cannot uniquely locate the wire protocol declaration")
    original_protocol = matches[0].group(1)
    if wire_protocol is not None:
        if not re.fullmatch(r"jam/\d+:[A-Za-z0-9:./_-]+", wire_protocol):
            raise ValueError("Invalid fixture wire protocol")
        if wire_protocol == original_protocol:
            raise ValueError("The incompatible fixture must use a different protocol")
        protocol_file.write_text(re.sub(
            declaration, f'pub const VERSION: &str = "{wire_protocol}";', protocol_source,
        ))
    if failing_daemon:
        # A deliberately broken, genuinely compiled release exercises installer
        # rollback after bundle replacement. This changes only the archived
        # candidate; production has no runtime failure switch.
        daemon = output / "crates/jam/src/bin/jamd.rs"
        daemon_source = daemon.read_text()
        entry = "    jam_core::server::serve()"
        if daemon_source.count(entry) != 1:
            raise ValueError("Cannot uniquely locate the daemon startup entry")
        daemon.write_text(daemon_source.replace(
            entry, '    anyhow::bail!("E2E candidate daemon startup failure")',
        ))
    if limited_capabilities:
        server = output / "crates/jam-core/src/server.rs"
        server_source = server.read_text()
        for name in ("clipboard_images", "workspace_creation_v1"):
            capability = f'                "{name}".into(),\n'
            if server_source.count(capability) != 1:
                raise ValueError(f"Cannot uniquely locate {name} capability")
            server_source = server_source.replace(capability, "")
        server.write_text(server_source)
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
        "wire_protocol": wire_protocol or original_protocol,
        "failing_daemon": failing_daemon,
        "limited_capabilities": limited_capabilities,
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
    parser.add_argument("--wire-protocol", help="Compile a real incompatible fixture variant")
    parser.add_argument("--failing-daemon", action="store_true",
                        help="Compile a broken candidate to exercise installed rollback")
    parser.add_argument("--limited-capabilities", action="store_true",
                        help="Compile a compatible server without the new client capabilities")
    args = parser.parse_args()
    print(
        json.dumps(
            prepare(
                args.source.resolve(),
                args.output.resolve(),
                args.version,
                args.cargo_target.resolve(),
                args.revision,
                args.wire_protocol,
                args.failing_daemon,
                args.limited_capabilities,
            ),
            indent=2,
        )
    )


if __name__ == "__main__":
    main()
