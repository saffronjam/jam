import json
import os
import pty
import socket
import subprocess
import sys
import tempfile
from pathlib import Path

import pytest

from jam_e2e.profile import Profile


def test_fixture_never_reuses_personal_profile(tmp_path, monkeypatch):
    monkeypatch.setenv("OPENAI_API_KEY", "must-not-inherit")
    monkeypatch.setenv("SSH_AUTH_SOCK", "/personal/agent")
    monkeypatch.setenv("XDG_RUNTIME_DIR", "/isolated-compositor")
    monkeypatch.setenv("WAYLAND_DISPLAY", "wayland-fixture")
    profile = Profile(tmp_path, Path("/bin/true"), Path("/bin/true"))
    assert "OPENAI_API_KEY" not in profile.env
    assert "SSH_AUTH_SOCK" not in profile.env
    assert profile.env["HOME"] == str(tmp_path / "home")
    assert profile.env["WAYLAND_DISPLAY"] == "/isolated-compositor/wayland-fixture"
    assert profile.env["XDG_RUNTIME_DIR"] == str(tmp_path / "run")
    assert profile.git("status", "--porcelain") == ""
    assert profile.socket.parent.stat().st_mode & 0o077 == 0
    assert profile.env["JAM_DEFAULTS_SUITE"].startswith("dev.saffronjam.e2e.")


def test_terminal_probe_observes_real_pty_input(tmp_path):
    from jam_e2e.driver import wait

    master, slave = pty.openpty()
    output = tmp_path / "result.json"
    probe = Path(__file__).parents[1] / "jam_e2e/terminal_probe.py"
    child = subprocess.Popen(
        [sys.executable, str(probe), str(output)], stdin=slave, stdout=slave, stderr=slave
    )
    try:
        wait(lambda: output.with_suffix(".ready").exists(), "raw probe startup")
        os.write(master, b"\x01x")
        child.wait(timeout=5)
        assert child.returncode == 0
        assert '"hex": "0178"' in output.read_text()
    finally:
        if child.poll() is None:
            child.kill()
            child.wait()
        os.close(master)
        os.close(slave)


def test_json_cli_error_never_counts_as_success(tmp_path, monkeypatch):
    profile = Profile(tmp_path, Path("/bin/true"), Path("/bin/true"))
    monkeypatch.setattr(
        subprocess,
        "run",
        lambda *args, **kwargs: subprocess.CompletedProcess(
            [], 0, stdout='{"Error":"pane is missing"}', stderr=""
        ),
    )
    with pytest.raises(AssertionError, match="pane is missing"):
        profile.call("pane", "send", "--pane", "missing", "--text", "probe")
    event = json.loads((tmp_path / "cli-events.jsonl").read_text())
    assert event["reply"] == {"Error": "pane is missing"}


def test_cleanup_accepts_an_already_reaped_owned_daemon():
    with tempfile.TemporaryDirectory(prefix="jhc-") as directory:
        profile = Profile(Path(directory), Path(sys.executable), Path(sys.executable))
        child = subprocess.Popen([sys.executable, "-c", "pass"])
        child.wait(timeout=5)
        with socket.socket(socket.AF_UNIX) as listener:
            listener.bind(str(profile.socket))
        (profile.socket.parent / "release.json").write_text(json.dumps({"pid": child.pid}))
        profile.stop()


@pytest.mark.parametrize("belongs", [True, False])
def test_failed_daemon_stop_cleans_only_the_owned_process(monkeypatch, belongs):
    with tempfile.TemporaryDirectory(prefix="jhc-") as directory:
        profile = Profile(Path(directory), Path(sys.executable), Path(sys.executable))
        child = subprocess.Popen(
            [sys.executable, "-c", "import sys; sys.stdin.read()"],
            env=dict(profile.env, JAM_SOCKET=str(profile.socket if belongs else "/foreign/socket")),
            stdin=subprocess.PIPE,
        )
        try:
            with socket.socket(socket.AF_UNIX) as listener:
                listener.bind(str(profile.socket))
            (profile.socket.parent / "release.json").write_text(json.dumps({"pid": child.pid}))

            def failed_stop(*args):
                raise RuntimeError("public stop failed")

            monkeypatch.setattr(profile, "call", failed_stop)
            with pytest.raises(RuntimeError, match="public stop failed") as failure:
                profile.stop()
            if belongs:
                child.wait(timeout=5)
                assert child.returncode != 0
                assert json.loads((profile.root / "cleanup.events").read_text())["pid"] == child.pid
            else:
                assert child.poll() is None, "Another profile's process was signalled"
                assert "Refusing to signal" in " ".join(failure.value.__notes__)
        finally:
            if child.poll() is None:
                child.kill()
                child.wait(timeout=5)
            child.stdin.close()
