"""Render the executable cases without requiring any product sources or artifacts."""

import argparse
from pathlib import Path

from .cases import CASES


def render():
    lines = [
        "# Desktop acceptance cases",
        "",
        f"{len(CASES)} cases; identical collection on macOS and Linux. No skips or expected failures.",
        "",
        "Each numbered step is performed externally. Fixture names identify required setup recipes, not alternate assertions.",
        "",
    ]
    for case in CASES:
        lines += [
            f"## {case.id}: {case.title}",
            "",
            f"Fixture: `{case.fixture or 'isolated local installation'}`.",
            "",
        ]
        for index, step in enumerate(case.steps, 1):
            lines.append(f"{index}. `{step!r}`")
        lines.append("")
    return "\n".join(lines)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true")
    options = parser.parse_args()
    path = Path(__file__).resolve().parents[1] / "CASES.md"
    expected = render()
    if options.check:
        if not path.exists() or path.read_text() != expected:
            raise SystemExit("CASES.md differs from the executable case catalog")
    else:
        path.write_text(expected)


if __name__ == "__main__":
    main()
