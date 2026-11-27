"""Own a private IBus/Mozc instance; never replace the user's input service."""

import json
import os
import signal
import subprocess
import sys
import threading
import time
from pathlib import Path


def owned_processes(root):
    marker = ("JAM_E2E_IME_ROOT=" + str(root)).encode()
    result = set()
    for entry in Path("/proc").iterdir():
        if not entry.name.isdigit() or int(entry.name) == os.getpid():
            continue
        try:
            if entry.stat().st_uid != os.getuid():
                continue
            if entry.joinpath("stat").read_text().rsplit(")", 1)[1].split()[0] == "Z":
                continue
            if marker in entry.joinpath("environ").read_bytes().split(b"\0"):
                result.add(int(entry.name))
        except (FileNotFoundError, PermissionError, ProcessLookupError):
            continue
    return result


def serve(root, ready):
    stopped = threading.Event()
    signal.signal(signal.SIGTERM, lambda *_: stopped.set())
    signal.signal(signal.SIGINT, lambda *_: stopped.set())
    if sys.platform == "darwin":
        ready.write_text(json.dumps({"native": "TIS", "pid": os.getpid()}))
        stopped.wait()
        return
    assert sys.platform == "linux", "Native Japanese input requires macOS or Linux"
    assert os.environ["IBUS_ADDRESS"] == "unix:path=" + str(root / "ibus.sock")
    for key in ("HOME", "XDG_CONFIG_HOME", "XDG_CACHE_HOME", "XDG_DATA_HOME"):
        assert Path(os.environ[key]).resolve().is_relative_to(root), "IBus needs its owned profile: " + key
    configuration = Path(os.environ["XDG_CONFIG_HOME"]) / "mozc/ibus_config.textproto"
    configuration.parent.mkdir(parents=True)
    # This config selects an actual Mozc engine and native Hiragana mode. It
    # does not produce text or substitute for the application's IM callbacks.
    configuration.write_text('engines { name: "mozc-jp" longname: "Mozc" layout: "us" '
                             'rank: 80 composition_mode: HIRAGANA }\n'
                             'active_on_launch: true\n')
    environment = dict(os.environ, JAM_E2E_IME_ROOT=str(root))

    def ibus(*args, check=True):
        return subprocess.run(["/usr/bin/ibus", *args], env=environment, check=check,
                              capture_output=True, text=True, timeout=5)

    daemon = subprocess.Popen(["/usr/bin/ibus-daemon", "--single", "--cache=refresh",
                               "--address", environment["IBUS_ADDRESS"]], env=environment)
    try:
        deadline = time.monotonic() + 20
        while True:
            assert daemon.poll() is None, "The owned IBus daemon exited before readiness"
            engines = ibus("list-engine", "--name-only", check=False)
            if engines.returncode == 0 and "mozc-jp" in engines.stdout.split():
                break
            assert time.monotonic() < deadline, "The real Mozc input engine did not become available: " + engines.stderr
            assert not stopped.wait(.1), "The native input fixture stopped during readiness"
        # The CLI's engine setter also changes X11 keyboard maps. Select the
        # actual IBus engine through its public API and leave KWin's map alone.
        engine = subprocess.run([
            "/usr/bin/python3", str(Path(__file__).with_name("ibus_engine.py")), "xkb:us::eng",
        ], env=environment, check=True, capture_output=True, text=True, timeout=10)
        assert json.loads(engine.stdout)["selected"] == "xkb:us::eng"
        ready.write_text(json.dumps({"native": "IBus/Mozc", "pid": daemon.pid,
                                     "address": environment["IBUS_ADDRESS"], "engines": engines.stdout.split()}))
        while not stopped.wait(.2):
            assert daemon.poll() is None, "The owned IBus daemon exited during the case"
    finally:
        before = owned_processes(root)
        errors = []
        if daemon.poll() is None:
            try:
                ibus("exit", check=False)
            except (OSError, subprocess.TimeoutExpired) as error:
                errors.append(str(error))
        for pid in before:
            try:
                os.kill(pid, signal.SIGTERM)
            except ProcessLookupError:
                pass
        deadline = time.monotonic() + 5
        while owned_processes(root) and time.monotonic() < deadline:
            stopped.wait(.05) if not stopped.is_set() else time.sleep(.05)
        remaining = owned_processes(root)
        for pid in remaining:
            try:
                os.kill(pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
        try:
            daemon.wait(timeout=5)
        except subprocess.TimeoutExpired:
            daemon.kill()
            daemon.wait(timeout=5)
            errors.append("IBus daemon required forced termination")
        (root / "fixture-ime-cleanup.json").write_text(json.dumps({
            "owned": sorted(before), "forced": sorted(remaining), "errors": errors,
        }))
        assert not remaining and not errors, "Owned native input cleanup failed: " + repr((remaining, errors))


if __name__ == "__main__":
    serve(Path(sys.argv[1]).resolve(strict=True), Path(sys.argv[2]))
