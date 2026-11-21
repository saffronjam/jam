"""Prepare disposable files and dependency behavior before launching Jam."""

import argparse
import base64
import json
import os
import subprocess
from pathlib import Path


def write(path, value):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, indent=2) + "\n")


def prepare(root, recipe):
    home = root / "home"
    repo = next(path for path in home.iterdir() if (path / ".git").exists())
    world = {
        "acknowledge": recipe not in ("delivery-unacknowledged", "accepted-task", "queued-task"),
        "hold_delivery": recipe == "delivery-pending",
        "report_result": recipe
        in ("task-results", "task-archive", "completed-task", "archived-task"),
        "helper_failure": recipe == "provider-launch-failed",
        "queue_failure": recipe == "failed-task",
        "hold_readiness": recipe == "waiting-agent",
    }
    write(home / "fixture-world.json", world)
    if recipe == "single-coordinator":
        (repo / "AGENTS.md").write_text("Fixture instruction: preserve user work.\n")
    # Install provider programs through the same standalone provisioner.
    subprocess.run(
        [
            os.sys.executable,
            str(Path(__file__).with_name("provision_provider.py")),
            str(root),
            "--world",
            json.dumps(world),
        ],
        check=True,
        timeout=10,
    )
    settings = []
    if recipe == "checkout-deletion-disabled":
        settings.append("allow_workspace_deletion = false")
    if recipe in ("stopped-coordinator", "codex-conversations", "provider-launch-failed"):
        settings.append("resume_interrupted_work = false")
    if settings:
        (root / "config/jam/config.toml").write_text("\n".join(settings) + "\n")
    if recipe.startswith("usage-") and recipe != "usage-no-sessions":
        claims = {"sub": "fixture-user", "email": "fixture@example.invalid"}
        if recipe == "usage-unknown-account":
            claims = {}
        encoded = base64.urlsafe_b64encode(json.dumps(claims).encode()).decode().rstrip("=")
        token = "fixture." + encoded + ".fixture"
        write(
            home / ".codex/auth.json",
            {
                "auth_mode": "chatgpt",
                "tokens": {
                    "access_token": token,
                    "id_token": token,
                    "account_id": "fixture-account",
                },
            },
        )
        write(
            home / ".claude/.credentials.json",
            {"claudeAiOauth": {"accessToken": "fixture-local-only"}},
        )
    if recipe == "provider-foreign":
        for path in (home / ".claude/skills/jam/SKILL.md", home / ".agents/skills/jam/SKILL.md"):
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text("User-owned foreign skill. Preserve these bytes.\n")
        (repo / "foreign-skill-marker").write_text(
            "User-owned foreign skill. Preserve these bytes.\n"
        )
    if recipe == "git-diff":
        # The fixture commit has two lines; replace both with three new lines.
        (repo / "tracked.txt").write_text("one\ntwo\nthree\n")
    if recipe in ("github-review", "gitlab-review"):
        provider = "github" if recipe == "github-review" else "gitlab"
        subprocess.run(
            ["git", "remote", "add", "origin", f"https://{provider}.com/fixture/project.git"],
            cwd=repo,
            check=True,
            capture_output=True,
            timeout=10,
        )
        tool = root / "bin" / ("gh" if provider == "github" else "glab")
        tool.write_text(
            "#!/usr/bin/env python3\n" + Path(__file__).with_name("review_provider.py").read_text()
        )
        tool.chmod(0o700)
    if recipe in ("existing-checkouts", "dirty-checkout", "checkout-deletion-disabled"):
        subprocess.run(
            ["git", "worktree", "add", "-b", "feature/existing", str(home / "Fixture checkout")],
            cwd=repo,
            check=True,
            capture_output=True,
            timeout=10,
        )
        if recipe == "dirty-checkout":
            (home / "Fixture checkout/dirty.txt").write_text("Keep this uncommitted work\n")
    scenario = {}
    if recipe.startswith("update-"):
        scenario["version"] = "0.0.5"
        if recipe == "update-bad-signature":
            scenario["bad_signature"] = True
        if recipe == "update-bad-checksum":
            scenario["bad_checksum"] = True
        if recipe == "update-download-failed":
            scenario["asset_error"] = 503
    if recipe == "usage-auth-error":
        scenario["http_error"] = 401
    if recipe == "usage-rate-limited":
        scenario["http_error"] = 429
    write(root / "service-world.json", scenario)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("root", type=Path)
    parser.add_argument("recipe")
    args = parser.parse_args()
    prepare(args.root, args.recipe)


if __name__ == "__main__":
    main()
