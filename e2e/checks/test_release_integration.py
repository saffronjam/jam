"""The fixture build must authenticate local HTTPS manifests through its normal updater."""

import json
import os
import subprocess
import sys
from pathlib import Path
from urllib.parse import urlsplit

from fixtures.prepare_trust import generate
from fixtures.services import load_config, signed_manifest
from jam_e2e.driver import wait


def test_fixture_release_trust_and_bad_signature(request, tmp_path):
    cli = request.config.getoption("--cli")
    bundle = request.config.getoption("--fixture-bundle")
    assert cli and bundle, "Supply a fixture-configured --cli and matching --fixture-bundle"
    bundle = Path(bundle).resolve().parent
    config = load_config(bundle / "trust/services.json")
    artifact = tmp_path / "jam-v0.0.5-linux-x86_64.tar.gz"
    artifact.write_bytes(b"Signature discovery fixture; installation is a separate test")
    releases = tmp_path / "releases"
    signed_manifest(
        releases / "v0.0.5",
        config,
        "0.0.5",
        "jam/7:ghostty/483ea3d:text",
        [{"path": str(artifact), "name": artifact.name, "platform": "linux", "arch": "x86_64"}],
    )
    scenario = tmp_path / "world.json"
    scenario.write_text('{"version":"0.0.5"}')
    ready = tmp_path / "https.ready"
    log = (tmp_path / "service.log").open("wb")
    process = subprocess.Popen(
        [
            sys.executable,
            str(bundle / "services.py"),
            "--config",
            str(bundle / "trust/services.json"),
            "--scenario",
            str(scenario),
            "--releases",
            str(releases),
            "--ready",
            str(ready),
        ],
        stdout=log,
        stderr=log,
    )
    try:
        wait(ready.exists, "fixture HTTPS service")
        command = [str(Path(cli).resolve()), "--json", "upgrade", "--check"]
        env = dict(os.environ, HOME=str(tmp_path))
        result = subprocess.run(
            command, env=env, text=True, capture_output=True, check=False, timeout=15
        )
        assert result.returncode == 0, result.stderr
        assert json.loads(result.stdout)["available"] is True
        scenario.write_text('{"version":"0.0.5","bad_signature":true}')
        result = subprocess.run(
            command, env=env, text=True, capture_output=True, check=False, timeout=15
        )
        assert result.returncode != 0
        assert "signature verification failed" in result.stderr.lower(), result.stderr
        assert (
            '"path": "/releases/download/v0.0.5/manifest.sig"'
            in ready.with_suffix(".events").read_text()
        )
        process.terminate()
        process.wait(timeout=5)
        ready.unlink()
        untrusted = tmp_path / "untrusted"
        generate(untrusted, port=urlsplit(config["origin"]).port)
        scenario.write_text('{"version":"0.0.5"}')
        process = subprocess.Popen(
            [
                sys.executable,
                str(bundle / "services.py"),
                "--config",
                str(untrusted / "services.json"),
                "--scenario",
                str(scenario),
                "--releases",
                str(releases),
                "--ready",
                str(ready),
            ],
            stdout=log,
            stderr=log,
        )
        wait(ready.exists, "untrusted fixture HTTPS service")
        result = subprocess.run(
            command, env=env, text=True, capture_output=True, check=False, timeout=15
        )
        assert result.returncode != 0
        assert "certificate" in result.stderr.lower(), result.stderr
    finally:
        process.terminate()
        process.wait(timeout=5)
        log.close()
