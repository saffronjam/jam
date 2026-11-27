"""Own a disposable systemd/OpenSSH container and private client configuration."""

import argparse
import json
import os
import signal
import subprocess
import threading
import time
import uuid
from pathlib import Path


def engine_environment():
    values = json.loads(os.environ["JAM_E2E_ENGINE_ENV"])
    allowed = {"HOME", "XDG_CONFIG_HOME", "XDG_DATA_HOME", "XDG_STATE_HOME", "XDG_RUNTIME_DIR"}
    if set(values) - allowed:
        raise ValueError("Unexpected container engine environment override")
    return dict(os.environ, **values)


def command(*args):
    environment = (
        engine_environment() if args[0] == os.environ.get("JAM_E2E_ENGINE") else os.environ
    )
    result = subprocess.run(
        args, env=environment, check=False, capture_output=True, text=True, timeout=30
    )
    if result.returncode:
        raise RuntimeError(f"External SSH fixture command failed: {args!r}\n{result.stderr}")
    return result.stdout.strip()


def serve(root, engine, image, ready):
    name = "jam-e2e-" + uuid.uuid4().hex
    stopped = threading.Event()
    signal.signal(signal.SIGTERM, lambda *_: stopped.set())
    signal.signal(signal.SIGINT, lambda *_: stopped.set())
    ssh = root / "home/.ssh"
    ssh.mkdir(mode=0o700)
    command("ssh-keygen", "-q", "-t", "ed25519", "-N", "", "-f", str(ssh / "id_ed25519"))
    # cgroups stay namespaced. The fixture requires a rootless Podman engine
    # with delegated cgroup v2; no host services or SSH configuration are used.
    try:
        command(
            engine,
            "run",
            "--detach",
            "--rm",
            "--pull=never",
            "--name",
            name,
            "--memory=256m",
            "--cpus=1",
            "--systemd=always",
            "--publish",
            "127.0.0.1::22",
            image,
        )
        deadline = time.monotonic() + 45
        while True:
            result = subprocess.run(
                [engine, "exec", name, "systemctl", "is-active", "ssh"],
                check=False,
                capture_output=True,
                text=True,
                env=engine_environment(),
                timeout=5,
            )
            if result.returncode == 0:
                break
            if time.monotonic() >= deadline:
                raise AssertionError(
                    "Container SSH/systemd did not become ready: " + result.stdout + result.stderr
                )
            if stopped.wait(0.2):
                raise InterruptedError("SSH fixture stopped during startup")
        command(
            engine,
            "exec",
            name,
            "install",
            "-d",
            "-m700",
            "-o",
            "fixture",
            "-g",
            "fixture",
            "/home/fixture/.ssh",
        )
        command(
            engine, "cp", str(ssh / "id_ed25519.pub"), name + ":/home/fixture/.ssh/authorized_keys"
        )
        command(
            engine, "exec", name, "chown", "fixture:fixture", "/home/fixture/.ssh/authorized_keys"
        )
        command(engine, "exec", name, "chmod", "600", "/home/fixture/.ssh/authorized_keys")
        port = command(engine, "port", name, "22/tcp").rsplit(":", 1)[1]
        host_key = command(engine, "exec", name, "cat", "/etc/ssh/ssh_host_ed25519_key.pub")
        (ssh / "known_hosts").write_text(f"[127.0.0.1]:{port} {host_key}\n")
        (ssh / "config").write_text(
            f"Host jam-fixture\n  HostName 127.0.0.1\n  Port {port}\n  User fixture\n"
            f"  IdentityFile {ssh / 'id_ed25519'}\n  IdentitiesOnly yes\n  IdentityAgent none\n"
            f"  UserKnownHostsFile {ssh / 'known_hosts'}\n  GlobalKnownHostsFile /dev/null\n"
            "  StrictHostKeyChecking yes\n  UpdateHostKeys no\n"
        )
        (ssh / "config").chmod(0o600)
        # Verify a real login and the user service manager before advertising readiness.
        command(
            "/usr/bin/ssh",
            "-F",
            str(ssh / "config"),
            "-oBatchMode=yes",
            "jam-fixture",
            "systemctl --user is-active default.target",
        )
        ready.write_text(json.dumps({"container": name, "destination": "jam-fixture"}))
        while not stopped.wait(1):
            if command(engine, "inspect", "--format", "{{.State.Running}}", name) != "true":
                raise AssertionError("Disposable SSH host exited during the scenario")
    finally:
        command(engine, "rm", "--force", "--ignore", name)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, required=True)
    parser.add_argument("--engine", required=True)
    parser.add_argument("--image", required=True)
    parser.add_argument("--ready", type=Path, required=True)
    args = parser.parse_args()
    serve(args.root, args.engine, args.image, args.ready)


if __name__ == "__main__":
    main()
