"""Create scenario setup through Jam's shipped CLI, never its database or RPC internals."""

import argparse
import json
import os
import subprocess
import time
from pathlib import Path

from ssh_service import engine_environment
from vm_service import host_command


def call(*args):
    try:
        result = subprocess.run(
            [os.environ["JAM_E2E_CLI"], "--json", *map(str, args)],
            check=True,
            text=True,
            capture_output=True,
            timeout=20,
        )
    except subprocess.CalledProcessError as error:
        error.add_note("Public CLI setup error: " + error.stderr)
        raise
    reply = json.loads(result.stdout)
    if isinstance(reply, dict) and ("Error" in reply or "error" in reply):
        raise AssertionError(f"Public CLI rejected fixture setup {args!r}: {reply}")
    return reply


def wait(check, description):
    deadline = time.monotonic() + 20
    while True:
        value = check()
        if value:
            return value
        if time.monotonic() >= deadline:
            raise AssertionError("Fixture setup timed out: " + description)
        time.sleep(0.1)


def seed(root, recipe):
    repo = next(path for path in (root / "home").iterdir() if (path / ".git").exists())
    created = call("workspace", "create", repo, "--name", "Fixture")
    workspace = created["State"]["workspaces"][0]["id"]
    ids = {"workspace": workspace}
    if recipe == "update-interrupted":
        from interrupt_upgrade import interrupt

        interrupt(root)
    if recipe in (
        "remote-connected",
        "remote-offline",
        "remote-persistence-disabled",
        "remote-auth-rejected",
        "remote-password-required",
        "remote-host-untrusted",
        "remote-update-available",
    ):
        ssh = [
            "/usr/bin/ssh",
            "-F",
            str(root / "home/.ssh/config"),
            "-oBatchMode=yes",
            "jam-fixture",
        ]
        subprocess.run(
            [*ssh, "mkdir -p /home/fixture/project && git -C /home/fixture/project init -b main"],
            check=True,
            capture_output=True,
            timeout=15,
        )
        remote = call("remote", "add", "Fixture", "jam-fixture")
        ids["remote"] = remote["id"]
        call("remote", "install", ids["remote"], os.environ["JAM_E2E_REMOTE_ARCHIVE"])
        remote_state = call(
            "--remote",
            ids["remote"],
            "workspace",
            "create",
            "/home/fixture/project",
            "--name",
            "Remote project",
        )
        ids["remote_workspace"] = remote_state["State"]["workspaces"][0]["id"]
        host = json.loads((root / "ssh.ready").read_text())

        def control(*args):
            if host.get("backend") == "qemu":
                return host_command(root, *args)
            return subprocess.run(
                [os.environ["JAM_E2E_ENGINE"], "exec", host["container"], *args],
                check=True,
                capture_output=True,
                timeout=15,
                env=engine_environment(),
            )

        if recipe == "remote-persistence-disabled":
            control("loginctl", "disable-linger", "fixture")
        if recipe == "remote-offline":
            control("systemctl", "stop", "ssh.socket", "ssh.service")
        if recipe in ("remote-auth-rejected", "remote-password-required", "remote-host-untrusted"):
            for filename in ("config", "known_hosts"):
                (root / ("ssh-healthy." + filename)).write_bytes(
                    (root / "home/.ssh" / filename).read_bytes()
                )
        if recipe == "remote-auth-rejected":
            # Keep the server, known host and original login key intact. Only
            # the client's configured identity becomes an unauthorized real key.
            ssh_dir = root / "home/.ssh"
            rejected = ssh_dir / "rejected_ed25519"
            subprocess.run(
                ["/usr/bin/ssh-keygen", "-q", "-t", "ed25519", "-N", "", "-f", str(rejected)],
                check=True,
                capture_output=True,
                timeout=10,
            )
            config = ssh_dir / "config"
            config.write_text(
                config.read_text().replace(str(ssh_dir / "id_ed25519"), str(rejected))
                + "  PreferredAuthentications publickey\n"
            )
        if recipe == "remote-password-required":
            config = root / "home/.ssh/config"
            config.write_text(config.read_text() + "  PreferredAuthentications password\n")
        if recipe == "remote-host-untrusted":
            (root / "home/.ssh/known_hosts").write_text("")
    if recipe in ("provider-installed", "provider-outdated"):
        call("skill", "setup", "--agent", "claude", "--agent", "codex")
        if recipe == "provider-outdated":
            for skill in (
                root / "home/.claude/skills/jam/SKILL.md",
                root / "home/.agents/skills/jam/SKILL.md",
            ):
                skill.write_text("Old managed Jam skill contents\n")
    if recipe in ("dirty-checkout", "checkout-deletion-disabled"):
        call("worktree", "open", "--workspace", workspace, root / "home/Fixture checkout")
    if recipe in (
        "queued-task",
        "waiting-agent",
        "accepted-task",
        "acknowledged-task",
        "failed-task",
        "completed-task",
        "archived-task",
    ):
        provider = "claude" if recipe == "waiting-agent" else "codex"
        owner = call("coordinate", "start", "--workspace", workspace, "--provider", provider)[
            "Coordination"
        ]["agent"]["id"]
        worker = call(
            "coordinate",
            "start",
            "--workspace",
            workspace,
            "--provider",
            provider,
            "--role",
            "worker",
            "--parent",
            owner,
        )["Coordination"]["agent"]["id"]
        ids.update(owner=owner, worker=worker)
        if recipe != "waiting-agent":
            wait(
                lambda: (
                    len(
                        [
                            a
                            for a in call("coordinate", "list")["Coordination"]["agents"]
                            if a.get("ready")
                        ]
                    )
                    == 2
                ),
                "task provider registration",
            )
        task = call("coordinate", "task", "--owner", owner, "Fixture task objective")[
            "Coordination"
        ]["id"]
        call("coordinate", "assign", "--task", task, "--agent", worker)
        ids["task"] = task
        if recipe == "queued-task":
            queued = call("coordinate", "task", "--owner", owner, "Queued fixture task")[
                "Coordination"
            ]["id"]
            call("coordinate", "assign", "--task", queued, "--agent", worker)
            ids["queued_task"] = queued
        elif recipe != "waiting-agent":

            def delivered():
                record = next(
                    t
                    for t in call("coordinate", "list")["Coordination"]["tasks"]
                    if t["id"] == task
                )
                if recipe in ("completed-task", "archived-task"):
                    return record.get("result") == "fixture-result"
                expected = {
                    "accepted-task": "transport_accepted",
                    "acknowledged-task": "acknowledged",
                    "failed-task": "delivery_failed",
                }[recipe]
                return record.get("delivery_status") == expected

            wait(delivered, "task fixture delivery state")
        if recipe == "archived-task":
            reviewer = call(
                "coordinate",
                "start",
                "--workspace",
                workspace,
                "--provider",
                provider,
                "--role",
                "reviewer",
                "--parent",
                owner,
            )["Coordination"]["agent"]["id"]
            review = call(
                "coordinate",
                "task",
                "--owner",
                owner,
                "--review-of",
                task,
                "Fixture independent review",
            )["Coordination"]["id"]
            call("coordinate", "assign", "--task", review, "--agent", reviewer)
            wait(
                lambda: any(
                    t["id"] == review and t["state"] == "done"
                    for t in call("coordinate", "list", "--all")["Coordination"]["tasks"]
                ),
                "fixture independent review completion",
            )
            call(
                "coordinate",
                "complete",
                "--task",
                task,
                "--agent",
                owner,
                "Fixture independent review evidence",
            )
    if recipe in (
        "stopped-coordinator",
        "codex-conversations",
        "provider-launch-failed",
        "single-coordinator",
    ) or recipe.startswith(("usage-", "delivery-", "task-")):
        providers = ("claude", "codex") if recipe == "usage-two-providers" else ("codex",)
        if recipe == "stopped-coordinator":
            providers = ("claude",)
        if recipe == "usage-no-sessions":
            providers = ()
        for provider in providers:
            agent = call(
                "coordinate",
                "start",
                "--workspace",
                workspace,
                "--provider",
                provider,
                "--role",
                "coordinator" if not recipe.startswith("usage-") else "worker",
            )["Coordination"]["agent"]
            ids[provider] = agent["id"]
            if recipe == "provider-launch-failed":
                wait(
                    lambda agent=agent: any(
                        p["id"] == agent["pane"] and p["exited"]
                        for p in call("workspace", "list")["State"]["panes"]
                    ),
                    "failed provider exit",
                )
            else:
                wait(
                    lambda agent=agent: any(
                        a["id"] == agent["id"] and a.get("ready")
                        for a in call("coordinate", "list")["Coordination"]["agents"]
                    ),
                    "provider session registration",
                )
            if recipe in ("stopped-coordinator", "codex-conversations"):
                call("pane", "send", "--pane", agent["pane"], "--text", "exit", "--enter")
                wait(
                    lambda agent=agent: any(
                        p["id"] == agent["pane"] and p["exited"]
                        for p in call("workspace", "list")["State"]["panes"]
                    ),
                    "stopped coordinator",
                )
    (root / "fixture-ids.json").write_text(json.dumps(ids, indent=2) + "\n")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("root", type=Path)
    parser.add_argument("recipe")
    args = parser.parse_args()
    seed(args.root, args.recipe)


if __name__ == "__main__":
    main()
