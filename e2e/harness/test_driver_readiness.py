"""Unavailable automation fails every case before expensive worlds can start."""

import json
import os
import shutil
import socket
import subprocess
import sys
from pathlib import Path
from xml.etree import ElementTree


def test_unavailable_driver_prevents_world_provisioning_without_skipping_cases(tmp_path):
    root = Path(__file__).parents[1]
    shutil.copyfile(root / "conftest.py", tmp_path / "conftest.py")
    executable = tmp_path / "unused-executable"
    executable.write_text("#!/bin/sh\nexit 1\n")
    executable.chmod(0o700)
    (tmp_path / "test_sample.py").write_text(
        "from types import SimpleNamespace\nimport pytest\n"
        "@pytest.fixture(params=[1, 2])\n"
        "def case(request): return SimpleNamespace(fixture='guarded')\n"
        "def test_case(jam): pass\n"
    )
    marker = tmp_path / "world-started"
    manifest = tmp_path / "manifest.json"
    manifest.write_text(json.dumps({"schema": 1, "recipes": {"guarded": {"setup": [[
        sys.executable, "-c",
        "from pathlib import Path; Path(" + repr(str(marker)) + ").touch(); raise SystemExit(1)",
    ]]}}}))
    # Keep ownership of an unlistened port: no unrelated service can race it.
    with socket.socket() as unavailable:
        unavailable.bind(("127.0.0.1", 0))
        port = unavailable.getsockname()[1]
        result = subprocess.run(
            [sys.executable, "-m", "pytest", "-q",
             "--rootdir", str(tmp_path), "--confcutdir", str(tmp_path),
             "--app", str(executable),
             "--cli", str(executable), "--daemon", str(executable),
             "--webdriver-url", f"http://127.0.0.1:{port}",
             "--fixture-bundle", str(manifest), "--junitxml", "results.xml"],
            cwd=tmp_path, env=dict(os.environ, PYTHONPATH=str(root), NO_PROXY="127.0.0.1"),
            capture_output=True, text=True, timeout=20,
        )
    assert result.returncode == 1, result.stdout + result.stderr
    assert "Native WebDriver is unavailable before fixture provisioning" in result.stdout
    assert not marker.exists(), "A world started despite unavailable native automation"
    suite = ElementTree.parse(tmp_path / "results.xml").getroot().find("testsuite")
    assert (suite.get("tests"), suite.get("errors"), suite.get("skipped")) == ("2", "2", "0")
