"""Check the independent observer's effective OpenSSH policy without a server."""

import os
import subprocess
from types import SimpleNamespace

import pytest

from jam_e2e import remote


@pytest.mark.parametrize("healthy", [False, True])
def test_observer_does_not_inherit_boot_poll_limits(tmp_path, monkeypatch, healthy):
    home = tmp_path / "home"
    ssh = home / ".ssh"
    ssh.mkdir(parents=True)
    config = ssh / "config"
    config.write_text(
        "Host jam-fixture\n  HostName 127.0.0.1\n  User fixture\n"
        "  ConnectTimeout 3\n  ConnectionAttempts 4\n"
        "  StrictHostKeyChecking yes\n  IdentitiesOnly yes\n"
    )
    (ssh / "known_hosts").touch()
    if healthy:
        (tmp_path / "ssh-healthy.config").write_text(config.read_text())
        (tmp_path / "ssh-healthy.known_hosts").touch()
        config.write_text("Host jam-fixture\n  HostName rejected.invalid\n")
    profile = SimpleNamespace(root=tmp_path, home=home, env=os.environ.copy())
    run = subprocess.run
    calls = []

    def inspect(argv, **kwargs):
        calls.append(kwargs)
        # OpenSSH itself resolves command-line/config precedence. -G prints
        # the effective policy without connecting or executing the command.
        return run([argv[0], "-G", *argv[1:]], **kwargs)

    monkeypatch.setattr(remote.subprocess, "run", inspect)
    effective = dict(line.split(None, 1) for line in remote.peer_command(
        profile, "printf observer-marker",
    ).splitlines())
    assert effective["connecttimeout"] == "10"
    assert effective["connectionattempts"] == "1"
    assert effective["hostname"] == "127.0.0.1"
    assert effective["stricthostkeychecking"] in {"true", "yes"}
    assert effective["batchmode"] in {"true", "yes"}
    assert effective["identitiesonly"] in {"true", "yes"}
    assert len(calls) == 1 and calls[0]["timeout"] == 20


def test_observer_ssh_failure_is_retained_without_retry(tmp_path, monkeypatch):
    profile = SimpleNamespace(root=tmp_path, home=tmp_path / "home", env={})
    failure = subprocess.CalledProcessError(
        255, ["ssh"], stderr="Connection timed out during banner exchange",
    )
    calls = []

    def reject(*args, **kwargs):
        calls.append((args, kwargs))
        raise failure

    monkeypatch.setattr(remote.subprocess, "run", reject)
    with pytest.raises(subprocess.CalledProcessError) as raised:
        remote.peer_command(profile, "printf observer-marker")
    assert raised.value is failure and len(calls) == 1
    assert (tmp_path / "fixture-ssh-observer.log").read_text() == failure.stderr + "\n"
