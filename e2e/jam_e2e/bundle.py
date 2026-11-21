"""Artifact recipes configure external dependencies, never application controllers."""

import json
import subprocess
import sys
import time
from pathlib import Path


class Resources:
    def __init__(self):
        self.processes = []
        self.logs = []

    def stop(self):
        failures = []
        for process in reversed(self.processes):
            if process.poll() is None:
                process.terminate()
                try:
                    process.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    process.kill()
                    process.wait(timeout=5)
                    failures.append("fixture service needed forced termination")
        for log in self.logs:
            log.close()
        if failures:
            raise AssertionError("; ".join(failures))


def expand(text, values):
    # JSON arguments, regexes and worktree templates may contain literal braces.
    for key, value in values.items():
        text = text.replace("{" + key + "}", value)
    return text


def prepare(manifest, recipe, profile):
    if not manifest:
        raise AssertionError(
            f"Required fixture bundle missing for {recipe}; supply --fixture-bundle"
        )
    path = Path(manifest).resolve()
    document = json.loads(path.read_text())
    if document.get("schema") != 1:
        raise AssertionError("Unsupported fixture manifest schema")
    if recipe not in document["recipes"]:
        raise AssertionError(f"Fixture bundle has no recipe {recipe!r}")
    spec = document["recipes"][recipe]
    values = {
        "home": str(profile.home),
        "root": str(profile.root),
        "repo": str(profile.repo),
        "config": str(profile.config),
        "socket": str(profile.socket),
        "cli": str(profile.cli),
        "daemon": str(profile.daemon),
        "bundle": str(path.parent),
        "python": sys.executable,
    }
    for key, value in spec.get("environment", {}).items():
        if key not in {
            "JAM_SOCKET",
            "JAM_STATE_DIR",
            "HOME",
            "XDG_CONFIG_HOME",
            "XDG_STATE_HOME",
            "XDG_DATA_HOME",
            "XDG_RUNTIME_DIR",
            "JAM_DEFAULTS_SUITE",
            "JAM_DAEMON",
            "JAM_E2E_CLI",
            "CODEX_HOME",
            "CLAUDE_CONFIG_DIR",
        }:
            profile.env[key] = expand(value, values)
        else:
            raise AssertionError(f"Fixture recipe cannot replace isolation setting {key}")
    resources = Resources()
    try:
        for command in spec.get("setup", []):
            subprocess.run(
                [expand(arg, values) for arg in command],
                cwd=profile.repo,
                env=profile.env,
                check=True,
                timeout=60,
                capture_output=True,
            )
        for index, service in enumerate(spec.get("services", [])):
            log = (profile.root / f"fixture-service-{index}.log").open("wb")
            resources.logs.append(log)
            process = subprocess.Popen(
                [expand(arg, values) for arg in service["argv"]],
                cwd=profile.repo,
                env=profile.env,
                stdin=subprocess.DEVNULL,
                stdout=log,
                stderr=log,
            )
            resources.processes.append(process)
            ready = Path(expand(service["ready"], values))
            deadline = time.monotonic() + 20
            while not ready.exists():
                if process.poll() is not None:
                    raise AssertionError(
                        f"Fixture service exited with {process.returncode}; see {log.name}"
                    )
                if time.monotonic() >= deadline:
                    raise AssertionError(f"Fixture service did not become ready; see {log.name}")
                time.sleep(0.05)
        for command in spec.get("after_services", []):
            subprocess.run(
                [expand(arg, values) for arg in command],
                cwd=profile.repo,
                env=profile.env,
                check=True,
                timeout=60,
                capture_output=True,
            )
        return resources
    except BaseException:
        resources.stop()
        raise
