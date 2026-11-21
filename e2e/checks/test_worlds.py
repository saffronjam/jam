"""Prove scenario worlds reach their requested state through installed artifacts."""

import json
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
        ("archived-task", "archived"),
        ("single-coordinator", "ready"),
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
                board = profile.call("coordinate", "list", "--all")["Coordination"]
                if expected == "result":
                    assert any(t.get("result") == "fixture-result" for t in board["tasks"])
                elif expected == "archived":
                    assert any(
                        t.get("archived")
                        and t["state"] == "done"
                        and t.get("result") == "fixture-result"
                        for t in board["tasks"]
                    )
                elif expected == "ready":
                    assert len(board["agents"]) == 1 and board["agents"][0]["ready"]
                else:
                    assert any(t.get("delivery_status") == expected for t in board["tasks"])
        finally:
            profile.stop()
            resources.stop()


@pytest.mark.parametrize(
    "provider,path", [("claude", ".claude/settings.json"), ("codex", ".codex/hooks.json")]
)
def test_custom_hook_world_preserves_the_user_document(request, provider, path):
    cli, daemon, manifest = (
        request.config.getoption(name) for name in ("--cli", "--daemon", "--fixture-bundle")
    )
    assert cli and daemon and manifest, "Supply fixture-configured artifacts and their bundle"
    with tempfile.TemporaryDirectory(prefix="jeh-") as directory:
        profile = Profile(Path(directory), Path(cli).resolve(), Path(daemon).resolve())
        resources = prepare(manifest, "provider-custom-hooks", profile)
        try:
            settings = profile.home / path
            original = json.loads(settings.read_text())
            assert (
                original["hooks"]["SessionStart"][0]["hooks"][0]["command"]
                == "printf fixture-user-hook"
            )
            installed = profile.call("skill", "setup", "--agent", provider)
            assert installed[0]["skill"] == "current" and installed[0]["hook"] == "current"
            updated = json.loads(settings.read_text())
            assert updated["fixture_user_setting"] == {"keep": True}
            assert original["hooks"]["SessionStart"][0] in updated["hooks"]["SessionStart"]
            profile.call("skill", "remove", "--agent", provider)
            assert json.loads(settings.read_text()) == original
        finally:
            profile.stop()
            resources.stop()
