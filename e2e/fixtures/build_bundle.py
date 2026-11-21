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
    "provider-custom-hooks",
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
    "update-interrupted",
)
REMOTE = (
    "remote-connected",
    "remote-offline",
    "remote-persistence-disabled",
    "remote-auth-rejected",
    "remote-password-required",
    "remote-host-untrusted",
)


def recipes(ssh_image=None, engine=None, vm=None, previous=False):
    values = {}
    remote_worlds = REMOTE + (("remote-update-available",) if previous else ())
    for recipe in LOCAL + HTTPS + (remote_worlds if ssh_image or vm else ()):
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
        if recipe in remote_worlds:
            archive = (
                "{bundle}/remote/previous.tar.gz"
                if recipe == "remote-update-available"
                else "{bundle}/remote/server.tar.gz"
            )
            if vm:
                spec["services"].insert(
                    0,
                    {
                        "argv": [
                            "{python}",
                            "{bundle}/vm_service.py",
                            "--root",
                            "{root}",
                            "--qemu",
                            vm["qemu"],
                            "--qemu-img",
                            vm["qemu_img"],
                            "--image",
                            vm["image"],
                            "--firmware",
                            vm["firmware"],
                            "--ready",
                            "{root}/ssh.ready",
                        ],
                        "ready": "{root}/ssh.ready",
                        "startup_timeout": 480,
                        "stop_timeout": 25,
                    },
                )
                spec["environment"] = {"JAM_E2E_REMOTE_ARCHIVE": archive}
                continue
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
                "JAM_E2E_REMOTE_ARCHIVE": archive,
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


def build(
    output,
    trust,
    releases=None,
    ssh_image=None,
    engine=None,
    remote_archive=None,
    vm=None,
    remote_previous_archive=None,
):
    if ssh_image and vm:
        raise ValueError("Choose exactly one owned SSH host backend")
    if remote_previous_archive and not (ssh_image or vm):
        raise ValueError("Previous remote archives require an owned SSH host backend")
    output.mkdir(parents=True, exist_ok=True)
    for source in Path(__file__).parent.glob("*.py"):
        shutil.copy2(source, output / source.name)
    shutil.copytree(trust, output / "trust", dirs_exist_ok=True)
    if releases:
        shutil.copytree(releases, output / "releases", dirs_exist_ok=True)
    else:
        (output / "releases").mkdir(exist_ok=True)
    if ssh_image or vm:
        if not remote_archive:
            raise ValueError("SSH worlds require a real --remote-archive")
        if ssh_image and (not engine or not Path(engine).is_absolute()):
            raise ValueError(
                "SSH worlds require an absolute rootless Podman --engine and --remote-archive"
            )
        if vm:
            for name, value in vm.items():
                if not Path(value).is_absolute() or not Path(value).is_file():
                    raise ValueError("Missing absolute VM resource: " + name)
            from fetch_vm_image import fetch

            fetch(Path(vm["image"]))
        (output / "remote").mkdir(exist_ok=True)
        shutil.copy2(remote_archive, output / "remote/server.tar.gz")
        if remote_previous_archive:
            import tarfile

            def build_info(archive):
                with tarfile.open(archive, "r:gz") as release:
                    candidates = [
                        member
                        for member in release.getmembers()
                        if member.name.removeprefix("./") == "manifest.json"
                    ]
                    assert len(candidates) == 1, "Real server archive requires its manifest"
                    return json.load(release.extractfile(candidates[0]))

            current = build_info(remote_archive)
            previous = build_info(remote_previous_archive)
            assert current["server_build"] != previous["server_build"], "Remote builds must differ"
            assert current["arch"] == previous["arch"], "Remote archive architectures must match"
            shutil.copy2(remote_previous_archive, output / "remote/previous.tar.gz")
            (output / "remote/builds.json").write_text(
                json.dumps(
                    {
                        "current": current["server_build"],
                        "previous": previous["server_build"],
                    }
                )
                + "\n"
            )
    (output / "manifest.json").write_text(
        json.dumps(recipes(ssh_image, engine, vm, bool(remote_previous_archive)), indent=2) + "\n"
    )


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("output", type=Path)
    parser.add_argument("--trust", type=Path, required=True)
    parser.add_argument("--releases", type=Path)
    parser.add_argument("--ssh-image")
    parser.add_argument("--engine")
    parser.add_argument("--remote-archive", type=Path)
    parser.add_argument("--remote-previous-archive", type=Path)
    parser.add_argument("--vm-image", type=Path)
    parser.add_argument("--qemu", type=Path)
    parser.add_argument("--qemu-img", type=Path)
    parser.add_argument("--vm-firmware", type=Path)
    args = parser.parse_args()
    vm = None
    resources = (args.vm_image, args.qemu, args.qemu_img, args.vm_firmware)
    if any(resources):
        if not all(resources):
            parser.error("VM worlds require --vm-image, --qemu, --qemu-img and --vm-firmware")
        vm = dict(
            zip(
                ("image", "qemu", "qemu_img", "firmware"),
                (str(path.resolve()) for path in resources),
            )
        )
    build(
        args.output,
        args.trust,
        args.releases,
        args.ssh_image,
        args.engine,
        args.remote_archive,
        vm,
        args.remote_previous_archive,
    )


if __name__ == "__main__":
    main()
