"""Own a headless real Linux SSH/systemd guest, including on macOS CI without nested VMs."""

import argparse
import json
import shlex
import signal
import socket
import subprocess
import sys
import threading
import time
import uuid
from pathlib import Path


def command(*args):
    return subprocess.run(
        list(map(str, args)), check=True, capture_output=True, text=True, timeout=30
    ).stdout.strip()


def seed_iso(directory, target):
    if sys.platform == "darwin":
        command(
            "/usr/bin/hdiutil",
            "makehybrid",
            "-iso",
            "-joliet",
            "-default-volume-name",
            "cidata",
            "-o",
            target,
            directory,
        )
    else:
        command(
            "xorriso",
            "-as",
            "mkisofs",
            "-volid",
            "cidata",
            "-joliet",
            "-rock",
            "-output",
            target,
            directory,
        )


def serve(root, qemu, qemu_img, image, firmware, ready):
    stopped = threading.Event()
    signal.signal(signal.SIGTERM, lambda *_: stopped.set())
    signal.signal(signal.SIGINT, lambda *_: stopped.set())
    guest = root / "vm"
    guest.mkdir(mode=0o700)
    ssh = root / "home/.ssh"
    ssh.mkdir(mode=0o700)
    for key in (ssh / "id_ed25519", guest / "host-key"):
        command("/usr/bin/ssh-keygen", "-q", "-t", "ed25519", "-N", "", "-f", key)
    seed = guest / "seed"
    seed.mkdir()
    boot_marker = "jam-e2e-ready-" + uuid.uuid4().hex
    console = root / "fixture-service-vm-console.log"
    config = {
        "hostname": "jam-fixture",
        "users": [
            {
                "name": "fixture",
                "shell": "/bin/bash",
                "lock_passwd": False,
                "sudo": "ALL=(ALL) NOPASSWD:ALL",
                "ssh_authorized_keys": [(ssh / "id_ed25519.pub").read_text().strip()],
            }
        ],
        "ssh_keys": {
            "ed25519_private": (guest / "host-key").read_text(),
            "ed25519_public": (guest / "host-key.pub").read_text(),
        },
        "ssh_pwauth": True,
        "chpasswd": {
            "expire": False,
            "users": [{"name": "fixture", "password": "fixture-local-only", "type": "text"}],
        },
        "packages": ["git", "socat"],
        "write_files": [
            {
                "path": "/etc/systemd/system/jam-fixture-https.service",
                "content": "[Unit]\nAfter=network-online.target\n[Service]\n"
                "ExecStart=/usr/bin/socat TCP-LISTEN:49651,bind=127.0.0.1,reuseaddr,fork TCP:10.0.2.2:49651\n"
                "Restart=on-failure\n[Install]\nWantedBy=multi-user.target\n",
            }
        ],
        "runcmd": [
            ["loginctl", "enable-linger", "fixture"],
            ["systemctl", "daemon-reload"],
            ["systemctl", "enable", "--now", "jam-fixture-https.service"],
            ["touch", "/var/lib/jam-fixture-ready"],
            ["sh", "-c", f"printf '\\n%s\\n' {boot_marker} > /dev/ttyAMA0"],
        ],
    }
    (seed / "user-data").write_text("#cloud-config\n" + json.dumps(config))
    (seed / "meta-data").write_text(json.dumps({"instance-id": "jam-" + uuid.uuid4().hex}))
    seed_iso(seed, guest / "seed.iso")
    overlay = guest / "disk.qcow2"
    command(qemu_img, "create", "-f", "qcow2", "-F", "qcow2", "-b", image, overlay)
    command(qemu_img, "resize", overlay, "+2G")
    with socket.socket() as reserve:
        reserve.bind(("127.0.0.1", 0))
        port = reserve.getsockname()[1]
    (ssh / "known_hosts").write_text(f"[127.0.0.1]:{port} " + (guest / "host-key.pub").read_text())
    (ssh / "config").write_text(
        f"Host jam-fixture\n  HostName 127.0.0.1\n  Port {port}\n  User fixture\n"
        f"  IdentityFile {ssh / 'id_ed25519'}\n  IdentitiesOnly yes\n  IdentityAgent none\n"
        f"  UserKnownHostsFile {ssh / 'known_hosts'}\n  GlobalKnownHostsFile /dev/null\n"
        "  StrictHostKeyChecking yes\n  UpdateHostKeys no\n  ConnectTimeout 3\n"
    )
    (ssh / "config").chmod(0o600)
    # TCG needs neither a privileged helper nor nested virtualization. Both
    # memory and CPU are bounded; the immutable base image is never written.
    process = subprocess.Popen(
        [
            str(qemu),
            "-machine",
            "virt,accel=tcg",
            "-cpu",
            "cortex-a72",
            "-smp",
            "1",
            "-m",
            "512",
            "-bios",
            str(firmware),
            "-display",
            "none",
            "-monitor",
            "none",
            "-no-reboot",
            "-boot",
            "strict=on",
            "-serial",
            "file:" + str(console),
            "-drive",
            "if=none,id=jam-os,format=qcow2,file=" + str(overlay),
            "-device",
            "virtio-blk-pci,drive=jam-os,bootindex=1",
            "-drive",
            "if=virtio,format=raw,readonly=on,file=" + str(guest / "seed.iso"),
            "-device",
            "virtio-rng-pci",
            "-nic",
            f"user,model=virtio-net-pci,hostfwd=tcp:127.0.0.1:{port}-:22",
        ],
        stdin=subprocess.DEVNULL,
    )
    try:
        deadline = time.monotonic() + 600
        # Each successful SSH handshake creates a PAM session in the guest.
        # Wait for cloud-init to finish installing packages through the serial
        # console before running the first authenticated readiness check.
        marker = boot_marker.encode()
        while not console.is_file() or marker not in console.read_bytes().splitlines():
            if process.poll() is not None:
                raise AssertionError(f"SSH guest exited during boot: {process.returncode}")
            if time.monotonic() >= deadline:
                raise AssertionError("Cloud-init did not report readiness on the guest serial console")
            if stopped.wait(2):
                raise InterruptedError("SSH guest stopped during boot")

        while True:
            if process.poll() is not None:
                raise AssertionError(f"SSH guest exited during boot: {process.returncode}")
            try:
                result = subprocess.run(
                    [
                        "/usr/bin/ssh",
                        "-F",
                        str(ssh / "config"),
                        "-oBatchMode=yes",
                        "-oConnectTimeout=10",
                        "jam-fixture",
                        "test -f /var/lib/jam-fixture-ready && systemctl --user is-active default.target",
                    ],
                    check=False,
                    capture_output=True,
                    text=True,
                    timeout=15,
                )
                if result.returncode == 0:
                    break
                last_error = result.stderr.strip() or "the guest user manager is not active"
            except subprocess.TimeoutExpired:
                last_error = "guest SSH handshake has not finished"
            if time.monotonic() >= deadline:
                raise AssertionError("SSH guest readiness failed: " + last_error)
            if stopped.wait(1):
                raise InterruptedError("SSH guest stopped during boot")
        ready.write_text(
            json.dumps({"backend": "qemu", "pid": process.pid, "destination": "jam-fixture"})
        )
        while not stopped.wait(1):
            if process.poll() is not None:
                raise AssertionError(f"SSH guest exited during the scenario: {process.returncode}")
    finally:
        if process.poll() is None:
            process.terminate()
            try:
                process.wait(timeout=15)
            except subprocess.TimeoutExpired:
                process.kill()
                process.wait(timeout=5)
                raise AssertionError("SSH guest required forced termination")


def host_command(root, *args):
    """Control only the owned guest OS; never invoke a Jam test controller."""
    return command(
        "/usr/bin/ssh",
        "-F",
        root / "home/.ssh/config",
        "-oBatchMode=yes",
        "jam-fixture",
        "sudo -n " + shlex.join(args),
    )


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("root", "qemu", "qemu-img", "image", "firmware", "ready"):
        parser.add_argument("--" + name, type=Path, required=True)
    args = parser.parse_args()
    serve(args.root, args.qemu, args.qemu_img, args.image, args.firmware, args.ready)
