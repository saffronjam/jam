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
        self.expected_exits = {}

    def stop(self):
        failures = []
        for process, stop_timeout in reversed(self.processes):
            if process.poll() is None:
                process.terminate()
                try:
                    process.wait(timeout=stop_timeout)
                except subprocess.TimeoutExpired:
                    process.kill()
                    process.wait(timeout=5)
                    failures.append("fixture service needed forced termination")
            if process in self.expected_exits and process.returncode != self.expected_exits[process]:
                failures.append(f"fixture service exited with {process.returncode}; expected {self.expected_exits[process]}")
        for log in self.logs:
            log.close()
        if failures:
            raise AssertionError("; ".join(failures))


def expand(text, values):
    # JSON arguments, regexes and worktree templates may contain literal braces.
    for key, value in values.items():
        text = text.replace("{" + key + "}", value)
    return text


def service_diagnostics(process, log):
    """Observe only the owned child, before cleanup destroys the evidence."""
    destination = Path(log.name).with_suffix(".diagnostics.log")
    commands = [["/bin/ps", "-ww", "-p", str(process.pid), "-o", "pid=,ppid=,stat=,etime=,args="]]
    if sys.platform == "darwin" and process.poll() is None:
        commands.append([
            "/usr/bin/sample", str(process.pid), "1", "1", "-file",
            str(Path(log.name).with_suffix(".sample.log")),
        ])
    with destination.open("w") as output:
        for command in commands:
            output.write("$ " + " ".join(command) + "\n")
            output.flush()
            try:
                subprocess.run(command, stdout=output, stderr=output, timeout=10, check=False)
            except (OSError, subprocess.TimeoutExpired) as error:
                output.write(str(error) + "\n")


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
            if value is None:
                profile.env.pop(key, None)
            else:
                profile.env[key] = expand(value, values)
        else:
            raise AssertionError(f"Fixture recipe cannot replace isolation setting {key}")
    resources = Resources()

    def run_setup(command, phase, index):
        log = profile.root / f"fixture-setup-{phase}-{index}.log"
        try:
            result = subprocess.run(
                [expand(arg, values) for arg in command],
                cwd=profile.repo,
                env=profile.env,
                check=True,
                timeout=60,
                capture_output=True,
            )
        except (subprocess.CalledProcessError, subprocess.TimeoutExpired) as error:
            log.write_bytes((error.stdout or b"") + b"\n" + (error.stderr or b""))
            error.add_note("External fixture setup output:\n" + log.read_text(errors="replace"))
            raise
        log.write_bytes(result.stdout + result.stderr)

    try:
        for index, command in enumerate(spec.get("setup", [])):
            run_setup(command, "before", index)
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
            resources.processes.append((process, service.get("stop_timeout", 5)))
            if "expected_exit" in service:
                resources.expected_exits[process] = service["expected_exit"]
            ready = Path(expand(service["ready"], values))
            deadline = time.monotonic() + service.get("startup_timeout", 20)
            while not ready.exists():
                if process.poll() is not None:
                    service_diagnostics(process, log)
                    raise AssertionError(
                        f"Fixture service exited with {process.returncode}; see {log.name}"
                    )
                if time.monotonic() >= deadline:
                    service_diagnostics(process, log)
                    raise AssertionError(f"Fixture service did not become ready; see {log.name}")
                time.sleep(0.05)
        for index, command in enumerate(spec.get("after_services", [])):
            run_setup(command, "after", index)
        return resources
    except BaseException as error:
        try:
            resources.stop()
        except Exception as cleanup_error:
            error.add_note(f"Fixture cleanup also failed: {cleanup_error}")
        raise
