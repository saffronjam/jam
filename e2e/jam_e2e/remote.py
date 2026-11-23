"""Independently observe the owned peer through its original, trusted SSH key."""

import json
import shlex
import subprocess
from pathlib import Path

from .driver import wait


def peer_command(profile, command):
    config = profile.root / "ssh-healthy.config"
    known_hosts = profile.root / "ssh-healthy.known_hosts"
    if not config.exists():
        config = profile.home / ".ssh/config"
        known_hosts = profile.home / ".ssh/known_hosts"
    try:
        result = subprocess.run([
            "/usr/bin/ssh", "-F", str(config), "-oBatchMode=yes",
            # The guest's short boot-poll timeout is not this observer's
            # handshake budget. Match the app while retaining the 20s command
            # deadline and exactly one connection attempt.
            "-oConnectTimeout=10", "-oConnectionAttempts=1",
            "-oUserKnownHostsFile=" + str(known_hosts), "jam-fixture", command,
        ], env=profile.env, check=True, capture_output=True, text=True, timeout=20)
    except subprocess.CalledProcessError as error:
        with (profile.root / "fixture-ssh-observer.log").open("a") as log:
            log.write(error.stderr + "\n")
        error.add_note("Independent peer SSH error: " + error.stderr)
        raise
    return result.stdout


def peer_call(profile, *args):
    command = ('export XDG_RUNTIME_DIR="${XDG_RUNTIME_DIR:-/run/user/$(id -u)}"; '
               'exec "$HOME/.local/share/jam/servers/current/jam" --json ' + shlex.join(args))
    value = json.loads(peer_command(profile, command))
    assert "Error" not in value and "error" not in value, value
    return value


def server_changed(profile, kind):
    """Observe a UI-requested daemon transition; the peer CLI never starts it."""
    ids = json.loads((profile.root / "fixture-ids.json").read_text())
    builds = json.loads((Path(profile.env["JAM_E2E_REMOTE_ARCHIVE"]).parent / "builds.json").read_text())
    expected = builds["current"] if kind in {"update", "takeover"} else ids["remote_build"]
    assert kind in {"update", "restart", "takeover"}
    if kind in {"update", "takeover"}:
        assert expected != ids["remote_build"], "Fixture must start with a different server build"

    def changed():
        try:
            info = peer_call(profile, "daemon", "info")["HostInfo"]
        except subprocess.CalledProcessError as error:
            # Loss of the socket/tunnel is an expected intermediate state of
            # this explicit restart. Authentication and other failures remain
            # immediate failures. Every stderr is retained in fixture logs.
            if any(text in error.stderr.lower() for text in (
                "connection refused", "connection reset", "connection closed", "no such file or directory",
                "failed to fill whole buffer",
            )):
                return False
            raise
        if info["build"] != expected or info["incarnation"] == ids["remote_incarnation"]:
            return False
        state = peer_call(profile, "workspace", "list")["State"]
        assert [w["id"] for w in state["workspaces"]] == [ids["remote_workspace"]]
        assert [tab["id"] for w in state["workspaces"] for tab in w["tabs"]] == ids["remote_tabs"]
        assert {pane["id"] for pane in state["panes"]} == set(ids["remote_sessions"])
        # The new daemon can publish saved layout before the UI has recovered
        # its shells. Never type into those stopped or still-initializing PTYs.
        for pane in state["panes"]:
            generation = pane.get("session_generation")
            if pane["exited"] or not generation or generation == ids["remote_sessions"][pane["id"]]:
                return False
            text = peer_call(profile, "pane", "capture", "--pane", pane["id"])["Capture"]
            if not text.rstrip().endswith("$"):
                return False
        info["restored_sessions"] = {pane["id"]: pane["session_generation"] for pane in state["panes"]}
        return info

    info = wait(changed, "real remote " + kind + " with preserved layout", timeout=120)
    (profile.root / "fixture-server-transition.log").write_text(json.dumps({
        "kind": kind, "before_build": ids["remote_build"],
        "before_incarnation": ids["remote_incarnation"], "after": info,
    }, indent=2))
