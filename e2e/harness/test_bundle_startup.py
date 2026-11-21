"""A stalled real dependency remains a failure and leaves owned-process evidence."""

import json
import os
import sys
from pathlib import Path
from types import SimpleNamespace

import pytest

from jam_e2e.bundle import prepare


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
