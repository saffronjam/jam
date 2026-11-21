"""Prove scenario worlds reach their requested state through installed artifacts."""

import tempfile
from pathlib import Path

import pytest

from jam_e2e.bundle import prepare
from jam_e2e.profile import Profile


@pytest.mark.parametrize(
    "recipe,expected",
    [
        ("queued-task", "queued"),
        ("waiting-agent", "waiting_for_agent"),
        ("accepted-task", "transport_accepted"),
        ("acknowledged-task", "acknowledged"),
        ("failed-task", "delivery_failed"),
        ("completed-task", "result"),
        ("stopped-coordinator", "stopped"),
        ("codex-conversations", "stopped"),
        ("provider-launch-failed", "launch_error"),
        ("dirty-checkout", "dirty"),
    ],
)
def test_prepared_world(request, recipe, expected):
    cli, daemon, manifest = (
        request.config.getoption(name) for name in ("--cli", "--daemon", "--fixture-bundle")
    )
    assert cli and daemon and manifest, "Supply installed artifacts and an external fixture bundle"
    with tempfile.TemporaryDirectory(prefix="jew-") as directory:
        profile = Profile(Path(directory), Path(cli).resolve(), Path(daemon).resolve())
        resources = prepare(manifest, recipe, profile)
        try:
            if expected == "dirty":
                assert (
                    profile.home / "Fixture checkout/dirty.txt"
                ).read_text() == "Keep this uncommitted work\n"
            elif expected in ("stopped", "launch_error"):
                panes = [p for p in profile.layout()["panes"] if p["agent_role"] == "coordinator"]
                assert len(panes) == 1 and panes[0]["exited"]
                if expected == "launch_error":
                    assert "fixture-launch-error" in panes[0]["launch_error"]
            else:
                board = profile.call("coordinate", "list")["Coordination"]
                if expected == "result":
                    assert any(t.get("result") == "fixture-result" for t in board["tasks"])
                else:
                    assert any(t.get("delivery_status") == expected for t in board["tasks"])
        finally:
            profile.stop()
            resources.stop()
