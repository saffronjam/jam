import json
import os
import shutil
import subprocess
import sys
from pathlib import Path
from xml.etree import ElementTree

import pytest

from jam_e2e.sharding import manifest, partition, reconcile


def reports(tmp_path):
    ids = ["A-001", "B-001", "C-001"]
    folders = []
    for index in range(2):
        folder = tmp_path / str(index)
        folder.mkdir()
        receipt = manifest(ids, index, 2, "macos", "source", "producer")
        (folder / "shard.json").write_text(json.dumps(receipt))
        root = ElementTree.Element("testsuite")
        for case in receipt["selected"]:
            ElementTree.SubElement(root, "testcase", name=f"test_user_journey[{case}]")
        ElementTree.ElementTree(root).write(folder / "acceptance.xml")
        folders.append(folder)
    return ids, folders


def test_complete_disjoint_shards_reconcile(tmp_path):
    ids, folders = reports(tmp_path)
    assert partition(ids, 0, 2) == ["A-001", "C-001"]
    assert reconcile(folders, ids, "macos", "source", "producer")["cases"] == 3


@pytest.mark.parametrize("corruption", ["missing", "duplicate", "source", "producer",
                                       "catalog", "selected", "error", "failure", "skipped",
                                       "missing-case", "duplicate-case"])
def test_incomplete_or_failed_shards_cannot_be_green(tmp_path, corruption):
    ids, folders = reports(tmp_path)
    if corruption == "missing":
        folders.pop()
    elif corruption == "duplicate":
        folders[1] = folders[0]
    elif corruption in {"source", "producer", "catalog", "selected"}:
        p = folders[0] / "shard.json"
        r = json.loads(p.read_text())
        r["revision" if corruption == "source" else corruption] = "wrong"
        p.write_text(json.dumps(r))
    else:
        p = folders[0] / "acceptance.xml"
        root = ElementTree.parse(p).getroot()
        if corruption == "missing-case":
            root.remove(root[0])
        elif corruption == "duplicate-case":
            ElementTree.SubElement(root, "testcase", name=root[0].get("name"))
        else:
            ElementTree.SubElement(root[0], corruption)
        ElementTree.ElementTree(root).write(p)
    with pytest.raises((ValueError, TypeError)):
        reconcile(folders, ids, "macos", "source", "producer")


@pytest.mark.parametrize("index,count", [(None, 2), (0, 0), (-1, 2), (2, 2), (0, 4)])
def test_invalid_partitions_fail(index, count):
    with pytest.raises((ValueError, TypeError)):
        partition(["A", "B", "C"], index, count)


def test_real_pytest_shards_execute_the_entire_missing_app_contract(tmp_path):
    from jam_e2e.cases import CASES

    source = Path(__file__).parents[1]
    copied = tmp_path / "suite"
    shutil.copytree(source, copied, ignore=shutil.ignore_patterns(
        "artifacts", ".venv", "harness", "__pycache__", ".pytest_cache", "upstream"))
    env = dict(os.environ, PYTHONPATH=str(copied))
    platform = "macos" if sys.platform == "darwin" else "linux"
    combined = []
    for index in range(2):
        folder = tmp_path / f"results-{index}"
        command = [sys.executable, "-m", "pytest", "-c", "pyproject.toml", "tests", "-q",
                   "--platform", platform, "--shard-index", str(index), "--shard-count", "2",
                   "--source-revision", "source", "--fixture-producer", "producer",
                   "--artifacts-dir", str(folder), "--junitxml", str(folder / "acceptance.xml")]
        result = subprocess.run(command, cwd=copied, env=env, capture_output=True,
                                text=True, timeout=30)
        assert result.returncode == 1, result.stdout + result.stderr
        root = ElementTree.parse(folder / "acceptance.xml").getroot()
        cases = list(root.iter("testcase"))
        receipt = json.loads((folder / "shard.json").read_text())
        assert [c.get("name").removeprefix("test_user_journey[").removesuffix("]")
                for c in cases] == receipt["selected"]
        assert all(c.find("error") is not None and "Missing application artifact" in
                   c.find("error").get("message", "") for c in cases)
        assert not list(root.iter("skipped"))
        combined.extend(receipt["selected"])
        if index == 0:
            for extra in (["-k", "TERM"], ["-m", "acceptance"], ["--allow-subset"]):
                rejected = subprocess.run(command + extra, cwd=copied, env=env,
                                          capture_output=True, text=True, timeout=30)
                assert rejected.returncode == 4, rejected.stdout + rejected.stderr
    assert len(combined) == len(set(combined)) == len(CASES)
    assert set(combined) == {c.id for c in CASES}
