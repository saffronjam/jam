"""Check reviewed behavior mappings; this is not a line-coverage metric."""

import argparse
import json
from pathlib import Path


def validate(ledger, cases):
    by_id = {case.id: case for case in cases}
    rows = ledger["behaviors"]
    if not rows or len({r["id"] for r in rows}) != len(rows):
        raise ValueError("Behavior IDs must be nonempty and unique")
    for row in rows:
        if not row["reviewed"] or not row["cases"] or not row["assertions"]:
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
    return len(rows)


def main():
    from .cases import CASES

    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true")
    parser.add_argument("--render", action="store_true")
    args = parser.parse_args()
    ledger = json.loads(Path(__file__).resolve().parents[1].joinpath("coverage.json").read_text())
    if args.render:
        lines = ["# New-main behavior coverage", "", "Scope: `ac5d161..004cf26`. Runtime proof is recorded separately.", "",
                 "Unreviewed mappings are incomplete, even when some scenarios exist.", "",
                 "| Behavior | Review | Shared cases |", "|---|---|---|"]
        for row in ledger["behaviors"]:
            lines.append(f"| {row['title']} | {'Reviewed' if row['reviewed'] else 'Pending'} | {', '.join(row['cases']) or 'Missing'} |")
        Path(__file__).resolve().parents[1].joinpath("COVERAGE.md").write_text("\n".join(lines) + "\n")
    if args.check:
        print(f"{validate(ledger, CASES)} reviewed behaviors mapped; native results required separately")
    if not args.render and not args.check:
        parser.error("Specify --check or --render")


if __name__ == "__main__":
    main()
