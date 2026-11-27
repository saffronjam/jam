"""Deterministic full-catalog partitions and strict evidence reconciliation."""

import argparse
import hashlib
import json
from pathlib import Path
from xml.etree import ElementTree


class AcceptanceFailed(ValueError):
    def __init__(self, report):
        super().__init__("Acceptance failures, errors and skips block publication")
        self.report = report


def partition(ids, index, count):
    if len(ids) != len(set(ids)) or not ids:
        raise ValueError("Catalog must be nonempty and unique")
    if type(index) is not int or type(count) is not int:
        raise ValueError("Shard indices and counts must be integers")
    if not 1 <= count <= len(ids) or not 0 <= index < count:
        raise ValueError("Invalid or empty shard")
    return ids[index::count]


def manifest(ids, index, count, platform, revision, producer):
    selected = partition(ids, index, count)
    if not revision or not producer:
        raise ValueError("Shard evidence requires source and fixture identities")
    return dict(schema=1, platform=platform, revision=revision, producer=producer,
                index=index, count=count, catalog=ids, selected=selected,
                catalog_sha256=hashlib.sha256(json.dumps(ids).encode()).hexdigest())


def reconcile(folders, expected, platform, revision, producer, count=2):
    """Require all cases exactly once, including setup/teardown success."""
    seen, indices = [], set()
    outcomes = dict(passed=0, failures=0, errors=0, skipped=0, missing_application=0)
    for folder in map(Path, folders):
        receipt = json.loads((folder / "shard.json").read_text())
        index = receipt["index"]
        if index in indices:
            raise ValueError("Duplicate shard")
        indices.add(index)
        wanted = manifest(expected, index, count, platform, revision, producer)
        if receipt != wanted:
            raise ValueError("Shard source, fixture, catalog or partition mismatch")
        root = ElementTree.parse(folder / "acceptance.xml").getroot()
        cases = list(root.iter("testcase"))
        ids = [c.get("name", "").removeprefix("test_user_journey[").removesuffix("]")
               for c in cases]
        if ids != receipt["selected"]:
            raise ValueError("JUnit cases differ from the assigned shard")
        for case in cases:
            failure = case.find("failure")
            error = case.find("error")
            skip = case.find("skipped")
            outcomes["failures"] += failure is not None
            outcomes["errors"] += error is not None
            outcomes["skipped"] += skip is not None
            outcomes["passed"] += all(outcome is None for outcome in (failure, error, skip))
            if error is not None and "Missing application artifact" in error.get("message", ""):
                outcomes["missing_application"] += 1
        seen.extend(ids)
    if indices != set(range(count)) or len(seen) != len(expected) or set(seen) != set(expected):
        raise ValueError("Missing, duplicate or incomplete catalog evidence")
    report = dict(platform=platform, cases=len(seen), revision=revision, producer=producer, **outcomes)
    if outcomes["passed"] != len(seen):
        raise AcceptanceFailed(report)
    return report


def main():
    from .cases import CASES

    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("folders", nargs="+")
    parser.add_argument("--platform", required=True, choices=("macos", "linux"))
    parser.add_argument("--revision", required=True)
    parser.add_argument("--producer", required=True)
    parser.add_argument("--count", type=int, default=2)
    args = parser.parse_args()
    print(json.dumps(reconcile(args.folders, [c.id for c in CASES], args.platform,
                               args.revision, args.producer, args.count)))


if __name__ == "__main__":
    main()
