"""A stalled real dependency remains a failure and leaves owned-process evidence."""

import json
import os
import sys
from pathlib import Path
from types import SimpleNamespace

import pytest

from jam_e2e.bundle import prepare


def test_a_native_service_cleanup_error_is_an_ordinary_failure(tmp_path):
    from jam_e2e.bundle import Resources

    import subprocess

    child = subprocess.Popen([sys.executable, "-c", "raise SystemExit(7)"])
    child.wait(timeout=5)
    resources = Resources()
    resources.processes.append((child, 1))
    resources.expected_exits[child] = 0
    with pytest.raises(AssertionError, match="exited with 7"):
        resources.stop()


def test_native_environment_override_does_not_change_the_parent_environment(tmp_path, monkeypatch):
    monkeypatch.setenv("IBUS_USE_PORTAL", "1")
    manifest = tmp_path / "manifest.json"
    manifest.write_text(json.dumps({"schema": 1, "recipes": {
        "native": {"environment": {"IBUS_USE_PORTAL": None}},
    }}))
    profile = SimpleNamespace(
        root=tmp_path, home=tmp_path, repo=tmp_path, config=tmp_path / "config.toml",
        socket=tmp_path / "socket", cli=Path(sys.executable), daemon=Path(sys.executable),
        env=dict(os.environ),
    )
    resources = prepare(manifest, "native", profile)
    try:
        assert "IBUS_USE_PORTAL" not in profile.env
        assert os.environ["IBUS_USE_PORTAL"] == "1"
    finally:
        resources.stop()


def test_service_startup_timeout_captures_the_child_before_cleanup(tmp_path):
    manifest = tmp_path / "manifest.json"
    manifest.write_text(json.dumps({
        "schema": 1,
        "recipes": {"stalled": {"services": [{
            "argv": [sys.executable, "-c", "import time; time.sleep(30)"],
            "ready": "{root}/never-ready",
            "startup_timeout": .1,
        }]}},
    }))
    profile = SimpleNamespace(
        root=tmp_path, home=tmp_path, repo=tmp_path, config=tmp_path / "config.toml",
        socket=tmp_path / "socket", cli=Path(sys.executable), daemon=Path(sys.executable),
        env=dict(os.environ),
    )
    with pytest.raises(AssertionError, match="did not become ready"):
        prepare(manifest, "stalled", profile)
    diagnostic = (tmp_path / "fixture-service-0.diagnostics.log").read_text()
    assert "time.sleep(30)" in diagnostic
    pid = int(diagnostic.splitlines()[1].split()[0])
    with pytest.raises(ProcessLookupError):
        os.kill(pid, 0)
    if sys.platform == "darwin":
        sample = (tmp_path / "fixture-service-0.sample.log").read_text()
        assert "Call graph" in sample
