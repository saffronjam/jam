"""Smoke checks for an unmodified installed release, separate from fixture builds."""

import pytest

from jam_e2e.cases import Case
from jam_e2e.steps import execute


@pytest.mark.parametrize(
    "case",
    [
        Case(
            "STOCK-001",
            "Installed release opens a working terminal",
            (("run", "printf installed-release-shell"), ("output", "installed-release-shell")),
        ),
        Case(
            "STOCK-002",
            "Installed release honors the launch directory",
            (("shell_cwd", "stock-launch-directory"),),
        ),
        Case(
            "STOCK-003",
            "Installed release retains its daemon across UI launches",
            (("remember_pids",), ("relaunch",), ("same_pids",)),
        ),
    ],
    ids=lambda case: case.id,
)
def test_stock_release(jam, case):
    memory = {}
    for step in case.steps:
        execute(jam, step, memory)
