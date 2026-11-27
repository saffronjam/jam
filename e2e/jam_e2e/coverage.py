"""Check reviewed behavior mappings; this is not a line-coverage metric."""

import argparse
import hashlib
import json
from pathlib import Path


def validate(ledger, cases):
    by_id = {case.id: case for case in cases}
    if len(by_id) != len(cases):
        raise ValueError("Case IDs must be unique")
    rows = ledger["behaviors"]
    if not rows or len({r["id"] for r in rows}) != len(rows):
        raise ValueError("Behavior IDs must be nonempty and unique")
    for row in rows:
        if (row["reviewed"] is not True or not row["cases"] or not row["assertions"]
                or any(not isinstance(assertion, str) or not assertion.strip()
                       for assertion in row["assertions"])):
            raise ValueError("Uncovered or unreviewed behavior: " + row["id"])
        if len(set(row["cases"])) != len(row["cases"]):
            raise ValueError("Duplicate case mapping: " + row["id"])
        for case_id in row["cases"]:
            if case_id not in by_id or row["id"] not in by_id[case_id].behavior_ids:
                raise ValueError("Missing or inconsistent case mapping: " + case_id)
    known = {r["id"] for r in rows}
    for case in cases:
        if not set(case.behavior_ids) <= known:
            raise ValueError("Case refers to an unknown behavior: " + case.id)
        for behavior in case.behavior_ids:
            row = next(r for r in rows if r["id"] == behavior)
            if case.id not in row["cases"]:
                raise ValueError("Case mapping must be bidirectional: " + case.id)
    changes = ledger["changed_files"]
    if len({r["path"] for r in changes}) != len(changes):
        raise ValueError("Duplicate changed-file classification")
    if not changes or any(not r["reviewed"] or not r["reason"] for r in changes):
        raise ValueError("Changed-file classification is incomplete")
    paths = sorted(row["path"] for row in changes)
    digest = hashlib.sha256("\n".join(paths).encode()).hexdigest()
    if digest != ledger["changed_paths_sha256"]:
        raise ValueError("Changed-file inventory differs from the reviewed source range")
    for row in changes:
        if row["classification"] not in {"ui", "backend", "test", "build", "copy", "documentation"}:
            raise ValueError("Unknown changed-file classification: " + row["path"])
        if not set(row["behaviors"]) <= known:
            raise ValueError("Changed file refers to an unknown behavior: " + row["path"])
        if row["classification"] in {"ui", "backend", "copy"} and not row["behaviors"]:
            raise ValueError("Changed behavior is unmapped: " + row["path"])
    return len(rows)


def render(ledger):
    source_range = f"{ledger['source_base'][:7]}..{ledger['source_tip'][:7]}"
    lines = ["# New-main behavior coverage", "",
             f"Scope: `{source_range}`. Runtime proof is recorded separately.", "",
             "Unreviewed mappings are incomplete, even when some scenarios exist.", "",
             "| Behavior | Review | Shared cases |", "|---|---|---|"]
    for row in ledger["behaviors"]:
        review = "Reviewed" if row["reviewed"] else "Pending"
        cases = ", ".join(row["cases"]) or "Missing"
        lines.append(f"| {row['title']} | {review} | {cases} |")
    return "\n".join(lines) + "\n"


def main():
    from .cases import CASES

    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true")
    parser.add_argument("--render", action="store_true")
    args = parser.parse_args()
    root = Path(__file__).resolve().parents[1]
    ledger = json.loads((root / "coverage.json").read_text())
    summary = root / "COVERAGE.md"
    if args.render:
        summary.write_text(render(ledger))
    if args.check:
        if summary.read_text() != render(ledger):
            raise SystemExit("COVERAGE.md differs from the reviewed behavior inventory")
        print(f"{validate(ledger, CASES)} reviewed behaviors mapped; native results required separately")
    if not args.render and not args.check:
        parser.error("Specify --check or --render")


if __name__ == "__main__":
    main()
