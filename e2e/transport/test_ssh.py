"""Exercise real OpenSSH and systemd persistence with an owned disposable host."""

import json
import re
import subprocess
import tempfile
from pathlib import Path

import pytest

from jam_e2e.bundle import prepare
from jam_e2e.driver import wait
from jam_e2e.profile import Profile


def test_real_ssh_reattaches_the_same_shell(request):
    cli, daemon, manifest = (
        request.config.getoption(name) for name in ("--cli", "--daemon", "--fixture-bundle")
    )
    assert cli and daemon and manifest, (
        "Supply fixture-configured artifacts and an SSH fixture bundle"
    )
    with tempfile.TemporaryDirectory(prefix="jer-") as directory:
        profile = Profile(Path(directory), Path(cli).resolve(), Path(daemon).resolve())
        resources = prepare(manifest, "remote-connected", profile)
        try:
            remote = json.loads((profile.root / "fixture-ids.json").read_text())["remote"]
            state = profile.call("--remote", remote, "workspace", "list")["State"]
            pane = state["panes"][0]["id"]
            profile.call(
                "--remote",
                remote,
                "pane",
                "send",
                "--pane",
                pane,
                "--text",
                "printf '\\nREMOTE_PID:%s\\n' \"$$\"",
                "--enter",
            )

            def pid():
                captured = profile.call("--remote", remote, "pane", "capture", "--pane", pane)[
                    "Capture"
                ]
                found = re.findall(r"^REMOTE_PID:(\d+)\s*$", captured, re.MULTILINE)
                return found[-1] if found else False

            original = wait(pid, "remote shell command")
            # Each invocation establishes and tears down its own real SSH tunnel.
            assert pid() == original
            assert pid() == original
            persistence = subprocess.run(
                [
                    "/usr/bin/ssh",
                    "-F",
                    str(profile.home / ".ssh/config"),
                    "-oBatchMode=yes",
                    "jam-fixture",
                    "loginctl show-user fixture -p Linger --value",
                ],
                env=profile.env,
                check=True,
                text=True,
                capture_output=True,
                timeout=15,
            )
            assert persistence.stdout.strip() == "yes"
        finally:
            profile.stop()
            resources.stop()


@pytest.mark.parametrize("recipe", ["remote-offline", "remote-persistence-disabled"])
def test_real_remote_failure_world(request, recipe):
    cli, daemon, manifest = (
        request.config.getoption(name) for name in ("--cli", "--daemon", "--fixture-bundle")
    )
    assert cli and daemon and manifest, (
        "Supply fixture-configured artifacts and an SSH fixture bundle"
    )
    with tempfile.TemporaryDirectory(prefix="jer-") as directory:
        profile = Profile(Path(directory), Path(cli).resolve(), Path(daemon).resolve())
        resources = prepare(manifest, recipe, profile)
        try:
            remote = json.loads((profile.root / "fixture-ids.json").read_text())["remote"]
            if recipe == "remote-offline":
                with pytest.raises(subprocess.CalledProcessError):
                    profile.call("--remote", remote, "workspace", "list")
            else:
                persistence = subprocess.run(
                    [
                        "/usr/bin/ssh",
                        "-F",
                        str(profile.home / ".ssh/config"),
                        "-oBatchMode=yes",
                        "jam-fixture",
                        "loginctl show-user fixture -p Linger --value",
                    ],
                    env=profile.env,
                    check=True,
                    text=True,
                    capture_output=True,
                    timeout=15,
                )
                assert persistence.stdout.strip() == "no"
        finally:
            profile.stop()
            resources.stop()
