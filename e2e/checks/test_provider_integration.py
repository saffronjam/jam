"""Validate external provider worlds against installed public CLI/daemon artifacts."""

import json
import subprocess
import sys
import tempfile
from pathlib import Path

import pytest

from jam_e2e.driver import wait
from jam_e2e.profile import Profile


@pytest.fixture
def live_profile(request):
    cli, daemon = (request.config.getoption(name) for name in ("--cli", "--daemon"))
    assert cli and daemon, "Supply --cli and --daemon installation artifacts"
    with tempfile.TemporaryDirectory(prefix="jec-") as directory:
        root = Path(directory)
        profile = Profile(root, Path(cli).resolve(), Path(daemon).resolve())
        provision = Path(__file__).parents[1] / "fixtures/provision_provider.py"
        subprocess.run(
            [sys.executable, str(provision), str(root), "--world", '{"report_result":true}'],
            check=True,
            capture_output=True,
            timeout=10,
        )
        with (root / "daemon.log").open("wb") as log:
            process = subprocess.Popen(
                [str(profile.daemon)], env=profile.env, stdout=log, stderr=log
            )
            try:
                wait(profile.socket.exists, "isolated daemon startup")
                yield profile
            finally:
                profile.stop()
                process.wait(timeout=10)


@pytest.mark.parametrize("provider", ["claude", "codex"])
def test_catalog_delivery_acknowledgement_and_result(live_profile, provider):
    profile = live_profile
    catalog = profile.call("coordinate", "models", "--provider", provider)["AgentModels"]
    assert catalog["default_model"] == "fixture-model"
    assert catalog["models"][0]["efforts"] == ["high"]
    workspace = profile.call("workspace", "create", str(profile.repo))["State"]["workspaces"][0][
        "id"
    ]
    owner = profile.call("coordinate", "start", "--workspace", workspace, "--provider", provider)[
        "Coordination"
    ]["agent"]["id"]
    worker = profile.call(
        "coordinate",
        "start",
        "--workspace",
        workspace,
        "--provider",
        provider,
        "--role",
        "worker",
        "--parent",
        owner,
    )["Coordination"]["agent"]["id"]

    def board():
        return profile.call("coordinate", "list", "--all")["Coordination"]

    wait(
        lambda: len([a for a in board()["agents"] if a.get("ready")]) == 2,
        "both provider sessions ready",
    )
    task = profile.call("coordinate", "task", "--owner", owner, "Deliver fixture result")[
        "Coordination"
    ]["id"]
    profile.call("coordinate", "assign", "--task", task, "--agent", worker)

    def reported():
        value = board()
        task_state = next(t for t in value["tasks"] if t["id"] == task)
        return value if task_state["state"] == "review" else False

    result = wait(reported, "provider delivery and result", timeout=20)
    assert any(
        message["to"] == worker and message["state"] == "acknowledged"
        for message in result["messages"]
    ), json.dumps(result)
    assert "fixture-result" in json.dumps(result)
    reviewer = profile.call(
        "coordinate",
        "start",
        "--workspace",
        workspace,
        "--provider",
        provider,
        "--role",
        "reviewer",
        "--parent",
        owner,
    )["Coordination"]["agent"]["id"]
    review = profile.call(
        "coordinate", "task", "--owner", owner, "--review-of", task, "Independently review result"
    )["Coordination"]["id"]
    profile.call("coordinate", "assign", "--task", review, "--agent", reviewer)
    wait(
        lambda: any(t["id"] == review and t["state"] == "done" for t in board()["tasks"]),
        "independent review",
        timeout=20,
    )
    profile.call(
        "coordinate", "complete", "--task", task, "--agent", owner, "Fixture review evidence"
    )
    assert any(t["id"] == task and t["state"] == "done" and t["archived"] for t in board()["tasks"])
