"""Assemble portable external dependency recipes with artifact-specific trust."""

import argparse
import json
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
    "queued-task",
    "waiting-agent",
    "accepted-task",
    "acknowledged-task",
    "failed-task",
    "completed-task",
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


def recipes():
    values = {}
    for recipe in LOCAL + HTTPS:
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
    return {"schema": 1, "recipes": values}


def build(output, trust, releases=None):
    output.mkdir(parents=True, exist_ok=True)
    for source in Path(__file__).parent.glob("*.py"):
        shutil.copy2(source, output / source.name)
    shutil.copytree(trust, output / "trust", dirs_exist_ok=True)
    if releases:
        shutil.copytree(releases, output / "releases", dirs_exist_ok=True)
    else:
        (output / "releases").mkdir(exist_ok=True)
    (output / "manifest.json").write_text(json.dumps(recipes(), indent=2) + "\n")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("output", type=Path)
    parser.add_argument("--trust", type=Path, required=True)
    parser.add_argument("--releases", type=Path)
    args = parser.parse_args()
    build(args.output, args.trust, args.releases)


if __name__ == "__main__":
    main()
