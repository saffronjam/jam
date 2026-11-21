"""Exercise policy in real child pytest runs, not mocks of the exit status."""

import os
import shutil
import subprocess
import sys
from pathlib import Path

import pytest


@pytest.mark.parametrize(
    "body,extra",
    [
        ("def test_case():\n assert False", []),
        ("def test_case():\n import pytest; pytest.skip('missing UI')", []),
        ("def test_case():\n import pytest; pytest.xfail('Linux incomplete')", []),
        ("import pytest\n@pytest.mark.skipif(True, reason='Linux')\ndef test_case(): pass", []),
        ("import pytest\n@pytest.mark.xfail\ndef test_case(): assert False", []),
        ("def test_case(): pass", ["-k", "not_a_case"]),
        ("", []),
    ],
)
def test_forbidden_success_paths_return_nonzero(tmp_path, body, extra):
    root = Path(__file__).parents[1]
    shutil.copyfile(root / "conftest.py", tmp_path / "conftest.py")
    (tmp_path / "test_sample.py").write_text(body)
    env = dict(os.environ, PYTHONPATH=str(root))
    result = subprocess.run(
        [sys.executable, "-m", "pytest", "-q", *extra],
        cwd=tmp_path,
        env=env,
        check=False,
        capture_output=True,
        text=True,
        timeout=30,
    )
    assert result.returncode != 0, result.stdout + result.stderr


def test_source_checkout_is_not_a_runtime_dependency(tmp_path):
    root = Path(__file__).parents[1]
    shutil.copytree(
        root / "jam_e2e", tmp_path / "jam_e2e", ignore=shutil.ignore_patterns("__pycache__")
    )
    result = subprocess.run(
        [sys.executable, "-c", "from jam_e2e.cases import CASES; assert len(CASES) >= 100"],
        cwd=tmp_path,
        env=dict(os.environ, PYTHONPATH=str(tmp_path)),
        check=False,
        capture_output=True,
        text=True,
        timeout=30,
    )
    assert result.returncode == 0, result.stderr
