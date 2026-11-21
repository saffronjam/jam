"""Exercise real OpenSSH and systemd persistence with an owned disposable host."""

import json
import re
import shutil
import subprocess
import tempfile
from pathlib import Path

import pytest

from jam_e2e.bundle import prepare
from jam_e2e.driver import wait
from jam_e2e.profile import Profile


@pytest.fixture
def remote_profile(request):
    cli, daemon, manifest = (
        request.config.getoption(name) for name in ("--cli", "--daemon", "--fixture-bundle")
    )
    assert cli and daemon and manifest, (
        "Supply fixture-configured artifacts and an SSH fixture bundle"
    )
    with tempfile.TemporaryDirectory(prefix="jer-") as directory:
        profile = Profile(Path(directory), Path(cli).resolve(), Path(daemon).resolve())
        resources = None
        try:
            resources = prepare(manifest, request.param, profile)
            yield profile
        finally:
            try:
                profile.stop()
            finally:
                try:
                    if resources:
                        resources.stop()
                finally:
                    output = Path(request.config.getoption("--artifacts-dir")) / request.node.name
                    output.mkdir(parents=True, exist_ok=True)
                    for source in (
                        *profile.root.glob("fixture-*.log"),
                        profile.root / "fixture-ids.json",
                        profile.root / "cli-events.jsonl",
                        profile.state / "jamd.log",
                    ):
                        if source.is_file():
                            shutil.copy2(source, output / source.name)


@pytest.mark.parametrize("remote_profile", ["remote-connected"], indirect=True)
def test_real_ssh_reattaches_the_same_shell(remote_profile):
    profile = remote_profile
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
        captured = profile.call("--remote", remote, "pane", "capture", "--pane", pane)["Capture"]
        found = re.findall(r"^REMOTE_PID:(\d+)\s*$", captured, re.MULTILINE)
        return found[-1] if found else False

    original = wait(pid, "remote shell command")
    # Each invocation establishes and tears down its own real SSH tunnel.
    assert pid() == original
    assert pid() == original
    assert linger(profile) == "yes"


def linger(profile):
    return subprocess.run(
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
    ).stdout.strip()


@pytest.mark.parametrize(
    "remote_profile", ["remote-offline", "remote-persistence-disabled"], indirect=True
)
def test_real_remote_failure_world(remote_profile, request):
    profile = remote_profile
    recipe = request.node.callspec.params["remote_profile"]
    remote = json.loads((profile.root / "fixture-ids.json").read_text())["remote"]
    if recipe == "remote-offline":
        with pytest.raises(subprocess.CalledProcessError):
            profile.call("--remote", remote, "workspace", "list")
    else:
        assert linger(profile) == "no"


@pytest.mark.parametrize(
    "remote_profile,error",
    [
        ("remote-auth-rejected", "Permission denied"),
        ("remote-password-required", "Permission denied"),
        ("remote-host-untrusted", "Host key verification failed"),
    ],
    indirect=["remote_profile"],
)
def test_real_ssh_authentication_failures_can_be_repaired(remote_profile, error):
    profile = remote_profile
    remote = json.loads((profile.root / "fixture-ids.json").read_text())["remote"]
    with pytest.raises(subprocess.CalledProcessError) as rejected:
        profile.call("--remote", remote, "workspace", "list")
    assert error in rejected.value.stderr
    # Restore the known-good private dependency settings. This repairs the
    # external login, without accepting a key obtained through network scanning.
    for filename in ("config", "known_hosts"):
        shutil.copyfile(
            profile.root / ("ssh-healthy." + filename), profile.home / ".ssh" / filename
        )
    expected = json.loads((profile.root / "fixture-ids.json").read_text())["remote_workspace"]
    state = profile.call("--remote", remote, "workspace", "list")["State"]
    assert [workspace["id"] for workspace in state["workspaces"]] == [expected]
