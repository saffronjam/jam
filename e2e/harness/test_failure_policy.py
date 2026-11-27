"""Exercise policy in real child pytest runs, not mocks of the exit status."""

import os
import shutil
import subprocess
import sys
from xml.etree import ElementTree
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
    from jam_e2e.cases import CASES

    root = Path(__file__).parents[1]
    copied = tmp_path / "suite"
    shutil.copytree(
        root, copied,
        ignore=shutil.ignore_patterns("harness", "artifacts", ".venv", "__pycache__",
                                     ".pytest_cache", ".ruff_cache", "upstream"),
    )
    env = dict(os.environ, PYTHONPATH=str(copied))
    result = subprocess.run(
        [sys.executable, "-c",
         "from pathlib import Path; from jam_e2e import cases, driver, steps; "
         "assert all(Path(m.__file__).resolve().is_relative_to(Path.cwd().resolve()) "
         "for m in (cases, driver, steps)); "
         "assert Path('native/macos/physical-keys.swift').is_file(); "
         "assert Path('fixtures/build_bundle.py').is_file()"],
        cwd=copied, env=env,
        check=False,
        capture_output=True,
        text=True,
        timeout=30,
    )
    assert result.returncode == 0, result.stderr
    collections = []
    for platform in ("linux", "macos"):
        result = subprocess.run(
            [sys.executable, "-m", "pytest", "-c", "pyproject.toml", "tests",
             "--collect-only", "-q", "--platform", platform],
            cwd=copied, env=env, capture_output=True, text=True, timeout=30,
        )
        assert result.returncode == 0, (result.stdout + result.stderr)[-4000:]
        items = [line for line in result.stdout.splitlines()
                 if line.startswith("tests/test_acceptance.py::")]
        assert len(items) == len(CASES)
        collections.append(items)
    assert collections[0] == collections[1]

    # This executes the normal artifact preflight without any display, driver,
    # fixture service, or product source in the extracted working tree.
    report = copied / "missing-app.xml"
    platform = "macos" if sys.platform == "darwin" else "linux"
    result = subprocess.run(
        [sys.executable, "-m", "pytest", "-c", "pyproject.toml", "tests", "-q",
         "--platform", platform, "--junitxml=" + str(report)],
        cwd=copied, env=env, capture_output=True, text=True, timeout=30,
    )
    assert result.returncode == 1, (result.stdout + result.stderr)[-4000:]
    suite = ElementTree.parse(report).getroot().find("testsuite")
    assert suite is not None
    assert int(suite.get("tests")) == int(suite.get("errors")) == len(CASES)
    assert int(suite.get("skipped")) == int(suite.get("failures")) == 0
    assert all("Missing application artifact" in error.get("message", "")
               for error in suite.iter("error"))
