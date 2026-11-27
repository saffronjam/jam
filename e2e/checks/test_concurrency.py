"""Concurrent public clients must keep the real daemon responsive and consistent."""

import tempfile
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path
from threading import Barrier

from jam_e2e.bundle import prepare
from jam_e2e.profile import Profile


def test_concurrent_layout_and_board_observers(request):
    cli, daemon, manifest = (
        request.config.getoption(name) for name in ("--cli", "--daemon", "--fixture-bundle")
    )
    assert cli and daemon and manifest, "Supply installed artifacts and an external fixture bundle"
    with tempfile.TemporaryDirectory(prefix="jeo-") as directory:
        profile = Profile(Path(directory), Path(cli).resolve(), Path(daemon).resolve())
        resources = prepare(manifest, "provider-missing", profile)
        try:
            initial = profile.layout()
            workspace_ids = {workspace["id"] for workspace in initial["workspaces"]}
            pane_ids = {pane["id"] for pane in initial["panes"]}
            assert len(workspace_ids) == len(pane_ids) == 1
            start = Barrier(4)

            def observe():
                start.wait(timeout=10)
                for _ in range(20):
                    layout = profile.layout()
                    assert {workspace["id"] for workspace in layout["workspaces"]} == workspace_ids
                    assert {pane["id"] for pane in layout["panes"]} == pane_ids
                    board = profile.call("coordinate", "list", "--all")["Coordination"]
                    assert board["agents"] == board["tasks"] == board["messages"] == []

            with ThreadPoolExecutor(max_workers=4) as clients:
                futures = [clients.submit(observe) for _ in range(4)]
                for future in futures:
                    future.result(timeout=90)
        finally:
            try:
                profile.stop()
            finally:
                resources.stop()
