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
    "usage-configured-login",
    "usage-auth-error",
    "usage-rate-limited",
    "update-available",
    "update-slow-prepare",
    "update-foreign-server",
    "update-bad-signature",
    "update-bad-checksum",
    "update-download-failed",
    "update-interrupted",
    "update-rollback",
)
REMOTE = (
    "remote-connected",
    "remote-creation-transport",
    "remote-long-sidebar",
    "remote-offline",
    "remote-persistence-disabled",
    "remote-auth-rejected",
    "remote-password-required",
    "remote-host-untrusted",
    "usage-remote-account",
    "usage-remote-other-account",
    "usage-remote-other-organization",
    "usage-remote-auth-error",
)


def recipes(ssh_image=None, engine=None, vm=None, previous=False, newer=False,
            incompatible=False, legacy=False, limited=False, unmanaged=False):
    values = {}
    remote_worlds = REMOTE + (("remote-update-available",) if previous else ())
    remote_worlds += (("update-required", "update-required-without-compatible-release") if newer else ())
    remote_worlds += (("remote-incompatible",) if incompatible else ())
    remote_worlds += (("remote-legacy", "remote-takeover-failure") if legacy else ())
    remote_worlds += (("remote-limited",) if limited else ())
    update_worlds = HTTPS + (("update-unmanaged",) if unmanaged else ())
    for recipe in LOCAL + update_worlds + (remote_worlds if ssh_image or vm else ()):
        values[recipe] = spec = {
            "setup": [["{python}", "{bundle}/provision_world.py", "{root}", recipe, "--repo", "{repo}"]],
            "services": [{"argv": ["{daemon}"], "ready": "{socket}"}],
            "after_services": [["{python}", "{bundle}/seed_world.py", "{root}", recipe, "--repo", "{repo}"]],
        }
        if recipe == "provider-missing":
            spec["setup"] = []
        if recipe == "update-foreign-server":
            spec["services"][0]["argv"] = ["{python}", "{bundle}/foreign_server.py",
                                           "{root}", "{daemon}"]
        if recipe == "update-unmanaged":
            spec["installations"] = {"macos": "releases/unmanaged/Jam-macos-arm64.zip"}
        if (recipe in update_worlds or recipe.startswith("update-required")
                or recipe.startswith("usage-remote-")):
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
            if recipe in {"remote-creation-transport", "remote-long-sidebar"}:
                spec["services"].insert(0, {
                    "argv": ["{python}", "{bundle}/ssh_transport.py", "{root}"],
                    "ready": "{root}/ssh-transport.ready",
                    "expected_exit": 0,
                })
            archive = (
                "{bundle}/remote/previous.tar.gz"
                if recipe == "remote-update-available"
                else "{bundle}/remote/newer.tar.gz"
                if recipe.startswith("update-required")
                else "{bundle}/remote/incompatible.tar.gz"
                if recipe == "remote-incompatible"
                else "{bundle}/remote/legacy.tar.gz"
                if recipe in {"remote-legacy", "remote-takeover-failure"}
                else "{bundle}/remote/limited.tar.gz"
                if recipe == "remote-limited"
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
                        "startup_timeout": 660,
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
    values["native-reduced-motion"] = {
        "setup": [["{python}", "{bundle}/native_preferences.py", "{root}"]],
    }
    values["native-japanese-input"] = {
        "environment": {"GTK_IM_MODULE": "ibus", "IBUS_ADDRESS": "unix:path={root}/ibus.sock",
                        "XDG_CACHE_HOME": "{root}/cache", "JAM_E2E_IME_ROOT": "{root}",
                        "IBUS_USE_PORTAL": None},
        "services": [{"argv": ["{python}", "{bundle}/native_ime_service.py", "{root}",
                               "{root}/ime.ready"],
                      "ready": "{root}/ime.ready", "startup_timeout": 30,
                      "stop_timeout": 20, "expected_exit": 0}],
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
    remote_next_archive=None,
    remote_incompatible_archive=None,
    remote_legacy_archive=None,
    remote_limited_archive=None,
):
    if ssh_image and vm:
        raise ValueError("Choose exactly one owned SSH host backend")
    variants = dict(previous=remote_previous_archive, newer=remote_next_archive,
                    incompatible=remote_incompatible_archive, legacy=remote_legacy_archive,
                    limited=remote_limited_archive)
    if any(variants.values()) and not (ssh_image or vm):
        raise ValueError("Additional remote archives require an owned SSH host backend")
    output.mkdir(parents=True, exist_ok=True)
    for source in Path(__file__).parent.iterdir():
        if source.suffix in {".py", ".c"}:
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
        if any(variants.values()):
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
            builds = {"current": current["server_build"]}
            receipts = {"current": current}
            for name, archive in variants.items():
                if archive is None:
                    continue
                other = build_info(archive)
                assert current["server_build"] != other["server_build"], "Remote builds must differ"
                assert current["arch"] == other["arch"], "Remote archive architectures must match"
                if name in {"previous", "newer", "limited"}:
                    assert other["protocol"] == current["protocol"], f"{name} must use the current protocol"
                if name in {"newer", "previous"}:
                    version = tuple(map(int, other["release_version"].split(".")))
                    current_version = tuple(map(int, current["release_version"].split(".")))
                    if name == "newer":
                        assert version > current_version, "Newer server must require a client upgrade"
                        builds["newer_version"] = other["release_version"]
                    else:
                        assert version < current_version, "Previous server must be an older release"
                if name in {"incompatible", "legacy"}:
                    assert other["protocol"] != current["protocol"], "Wire protocols must differ"
                    if name == "legacy":
                        assert other["protocol"].startswith("jam/7:"), "Expected a published protocol-v7 server"
                    builds[f"{name}_protocol"] = other["protocol"]
                builds[name] = other["server_build"]
                receipts[name] = other
                shutil.copy2(archive, output / f"remote/{name}.tar.gz")
            builds["archives"] = receipts
            (output / "remote/builds.json").write_text(json.dumps(builds) + "\n")
    (output / "manifest.json").write_text(
        json.dumps(recipes(ssh_image, engine, vm,
                           unmanaged=(output / "releases/unmanaged/Jam-macos-arm64.zip").is_file(),
                           **{name: bool(path) for name, path in variants.items()}),
                   indent=2) + "\n"
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
    parser.add_argument("--remote-next-archive", type=Path)
    parser.add_argument("--remote-incompatible-archive", type=Path)
    parser.add_argument("--remote-legacy-archive", type=Path)
    parser.add_argument("--remote-limited-archive", type=Path)
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
        args.remote_next_archive,
        args.remote_incompatible_archive,
        args.remote_legacy_archive,
        args.remote_limited_archive,
    )


if __name__ == "__main__":
    main()
