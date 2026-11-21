"""Assemble portable external dependency recipes with artifact-specific trust."""

import argparse
import json
import os
import shutil
from pathlib import Path

LOCAL = (
    "provider-missing",
    "provider-detected",
    "provider-installed",
    "provider-outdated",
    "provider-foreign",
    "stopped-coordinator",
    "codex-conversations",
    "provider-launch-failed",
    "dirty-checkout",
    "checkout-deletion-disabled",
    "existing-checkouts",
    "git-diff",
    "github-review",
    "gitlab-review",
    "queued-task",
    "waiting-agent",
    "accepted-task",
    "acknowledged-task",
    "failed-task",
    "completed-task",
    "archived-task",
    "single-coordinator",
)
HTTPS = (
    "usage-two-providers",
    "usage-weekly",
    "usage-unknown-account",
    "usage-no-sessions",
    "usage-auth-error",
    "usage-rate-limited",
    "update-available",
    "update-bad-signature",
    "update-bad-checksum",
    "update-download-failed",
)
REMOTE = ("remote-connected", "remote-offline", "remote-persistence-disabled")


def recipes(ssh_image=None, engine=None):
    values = {}
    for recipe in LOCAL + HTTPS + (REMOTE if ssh_image else ()):
        values[recipe] = spec = {
            "setup": [["{python}", "{bundle}/provision_world.py", "{root}", recipe]],
            "services": [{"argv": ["{daemon}"], "ready": "{socket}"}],
            "after_services": [["{python}", "{bundle}/seed_world.py", "{root}", recipe]],
        }
        if recipe == "provider-missing":
            spec["setup"] = []
        if recipe in HTTPS:
            spec["services"].insert(
                0,
                {
                    "argv": [
                        "{python}",
                        "{bundle}/services.py",
                        "--config",
                        "{bundle}/trust/services.json",
                        "--scenario",
                        "{root}/service-world.json",
                        "--releases",
                        "{bundle}/releases",
                        "--ready",
                        "{root}/https.ready",
                    ],
                    "ready": "{root}/https.ready",
                },
            )
        if recipe in REMOTE:
            spec["services"].insert(
                0,
                {
                    "argv": [
                        "{python}",
                        "{bundle}/ssh_service.py",
                        "--root",
                        "{root}",
                        "--engine",
                        engine,
                        "--image",
                        ssh_image,
                        "--ready",
                        "{root}/ssh.ready",
                    ],
                    "ready": "{root}/ssh.ready",
                    "startup_timeout": 60,
                    "stop_timeout": 35,
                },
            )
            spec["environment"] = {
                "JAM_E2E_ENGINE": engine,
                "JAM_E2E_REMOTE_ARCHIVE": "{bundle}/remote/server.tar.gz",
                "JAM_E2E_ENGINE_ENV": json.dumps(
                    {
                        "HOME": os.environ["HOME"],
                        "XDG_CONFIG_HOME": os.environ.get(
                            "XDG_CONFIG_HOME", os.environ["HOME"] + "/.config"
                        ),
                        "XDG_DATA_HOME": os.environ.get(
                            "XDG_DATA_HOME", os.environ["HOME"] + "/.local/share"
                        ),
                        "XDG_STATE_HOME": os.environ.get(
                            "XDG_STATE_HOME", os.environ["HOME"] + "/.local/state"
                        ),
                        "XDG_RUNTIME_DIR": os.environ.get(
                            "XDG_RUNTIME_DIR", f"/run/user/{os.getuid()}"
                        ),
                    }
                ),
            }
    return {"schema": 1, "recipes": values}


def build(output, trust, releases=None, ssh_image=None, engine=None, remote_archive=None):
    output.mkdir(parents=True, exist_ok=True)
    for source in Path(__file__).parent.glob("*.py"):
        shutil.copy2(source, output / source.name)
    shutil.copytree(trust, output / "trust", dirs_exist_ok=True)
    if releases:
        shutil.copytree(releases, output / "releases", dirs_exist_ok=True)
    else:
        (output / "releases").mkdir(exist_ok=True)
    if ssh_image:
        if not engine or not Path(engine).is_absolute() or not remote_archive:
            raise ValueError(
                "SSH worlds require an absolute rootless Podman --engine and --remote-archive"
            )
        (output / "remote").mkdir(exist_ok=True)
        shutil.copy2(remote_archive, output / "remote/server.tar.gz")
    (output / "manifest.json").write_text(json.dumps(recipes(ssh_image, engine), indent=2) + "\n")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("output", type=Path)
    parser.add_argument("--trust", type=Path, required=True)
    parser.add_argument("--releases", type=Path)
    parser.add_argument("--ssh-image")
    parser.add_argument("--engine")
    parser.add_argument("--remote-archive", type=Path)
    args = parser.parse_args()
    build(args.output, args.trust, args.releases, args.ssh_image, args.engine, args.remote_archive)


if __name__ == "__main__":
    main()
