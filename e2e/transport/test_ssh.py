"""Exercise real OpenSSH and systemd persistence with an owned disposable host."""

import json
import re
import shlex
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
    from jam_e2e.remote import peer_call

    blocked = {name: (profile.home / ".ssh" / name).read_bytes()
               for name in ("config", "known_hosts")}
    ids = json.loads((profile.root / "fixture-ids.json").read_text())
    info = peer_call(profile, "daemon", "info")["HostInfo"]
    assert (info["build"], info["incarnation"]) == (ids["remote_build"], ids["remote_incarnation"])
    assert [w["id"] for w in peer_call(profile, "workspace", "list")["State"]["workspaces"]] == [ids["remote_workspace"]]
    assert all((profile.home / ".ssh" / name).read_bytes() == value for name, value in blocked.items())
    # Restore the known-good private dependency settings. This repairs the
    # external login, without accepting a key obtained through network scanning.
    for filename in ("config", "known_hosts"):
        shutil.copyfile(
            profile.root / ("ssh-healthy." + filename), profile.home / ".ssh" / filename
        )
    expected = json.loads((profile.root / "fixture-ids.json").read_text())["remote_workspace"]
    state = profile.call("--remote", remote, "workspace", "list")["State"]
    assert [workspace["id"] for workspace in state["workspaces"]] == [expected]


@pytest.mark.parametrize("remote_profile", ["remote-update-available"], indirect=True)
def test_real_remote_update_defers_while_a_shell_is_running(remote_profile, request):
    profile = remote_profile
    ids = json.loads((profile.root / "fixture-ids.json").read_text())
    bundle = Path(request.config.getoption("--fixture-bundle")).resolve().parent
    builds = json.loads((bundle / "remote/builds.json").read_text())
    old = profile.call("--remote", ids["remote"], "daemon", "info")["HostInfo"]
    assert old["build"] == builds["previous"] != builds["current"]
    with pytest.raises(subprocess.CalledProcessError) as deferred:
        profile.call("remote", "install", ids["remote"], str(bundle / "remote/server.tar.gz"))
    assert "Running sessions prevent the server update" in deferred.value.stderr
    current = profile.call("--remote", ids["remote"], "daemon", "info")["HostInfo"]
    assert current["build"] == old["build"] and current["incarnation"] == old["incarnation"]
    state = profile.call("--remote", ids["remote"], "workspace", "list")["State"]
    assert [workspace["id"] for workspace in state["workspaces"]] == [ids["remote_workspace"]]


def ssh_cli(profile, *args):
    """Observe the installed server using its own public CLI after incompatibility."""
    result = subprocess.run(
        ["/usr/bin/ssh", "-F", str(profile.home / ".ssh/config"), "-oBatchMode=yes",
         "jam-fixture", 'export XDG_RUNTIME_DIR="${XDG_RUNTIME_DIR:-/run/user/$(id -u)}"; '
         'exec "$HOME/.local/share/jam/servers/current/jam" --json ' + shlex.join(args)],
        env=profile.env, check=True, capture_output=True, text=True, timeout=20,
    )
    value = json.loads(result.stdout)
    assert "Error" not in value and "error" not in value, value
    return value


@pytest.mark.parametrize("remote_profile", ["remote-incompatible"], indirect=True)
def test_real_incompatible_protocol_is_rejected_without_restarting_the_server(remote_profile, request):
    profile = remote_profile
    ids = json.loads((profile.root / "fixture-ids.json").read_text())
    bundle = Path(request.config.getoption("--fixture-bundle")).resolve().parent
    builds = json.loads((bundle / "remote/builds.json").read_text())
    before = ssh_cli(profile, "daemon", "info")["HostInfo"]
    assert before["build"] == builds["incompatible"] != builds["current"]
    with pytest.raises(subprocess.CalledProcessError) as rejected:
        profile.call("--remote", ids["remote"], "workspace", "list")
    assert "incompatible jam daemon" in rejected.value.stderr.lower()
    after = ssh_cli(profile, "daemon", "info")["HostInfo"]
    assert (after["build"], after["incarnation"]) == (before["build"], before["incarnation"])
    state = ssh_cli(profile, "workspace", "list")["State"]
    assert [workspace["id"] for workspace in state["workspaces"]] == [ids["remote_workspace"]]
    assert {pane["id"]: pane["session_generation"] for pane in state["panes"]} == ids["remote_sessions"]


@pytest.mark.parametrize("remote_profile", ["update-required"], indirect=True)
def test_real_newer_server_world_preserves_its_running_workspace(remote_profile, request):
    profile = remote_profile
    ids = json.loads((profile.root / "fixture-ids.json").read_text())
    bundle = Path(request.config.getoption("--fixture-bundle")).resolve().parent
    builds = json.loads((bundle / "remote/builds.json").read_text())
    before = ssh_cli(profile, "daemon", "info")["HostInfo"]
    assert before["build"] == builds["newer"]
    state = ssh_cli(profile, "workspace", "list")["State"]
    assert [workspace["id"] for workspace in state["workspaces"]] == [ids["remote_workspace"]]
    assert state["panes"], "The newer server must have an actual running terminal"
    assert ssh_cli(profile, "daemon", "info")["HostInfo"]["build"] == builds["newer"]
    # This check proves the external dependency, not the GUI's upgrade gate.
    # The ordinary CLI directly connects to compatible wire protocols; native
    # UPDATE-002 must exercise the desktop's version check and required prompt.
    version = subprocess.run(
        [str(profile.cli), "--version"], check=True, capture_output=True, text=True, timeout=10,
    ).stdout.strip().removeprefix("jam ")
    numeric = lambda value: tuple(map(int, value.split(".")))
    assert numeric(version) < numeric(builds["newer_version"])
    after = ssh_cli(profile, "daemon", "info")["HostInfo"]
    assert (after["build"], after["incarnation"]) == (before["build"], before["incarnation"])
    current = ssh_cli(profile, "workspace", "list")["State"]
    (profile.root / "fixture-newer-server.log").write_text(json.dumps({
        "before": {"info": before, "state": state},
        "after": {"info": after, "state": current},
    }, indent=2))

    def identity(snapshot):
        # Repository discovery settles asynchronously; preserve the actual
        # workspace layout and PTY generations rather than transient metadata.
        return {
            "workspaces": [(workspace["id"], workspace["name"], workspace["path"],
                            workspace["tabs"]) for workspace in snapshot["workspaces"]],
            "panes": [(pane["id"], pane["session_generation"], pane["exited"])
                      for pane in snapshot["panes"]],
        }

    assert identity(current) == identity(state)
