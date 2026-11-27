import ast
import json
import os
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import Mock

import pytest

from jam_e2e.cases import CASES
from jam_e2e.driver import literal, wait
from jam_e2e.steps import execute


def test_unique_cases_have_assertions():
    assert len(CASES) >= 80
    assert len({case.id for case in CASES}) == len(CASES)
    for case in CASES:
        assert case.steps
        assert any(
            step[0]
            in {
                "output",
                "output_repo",
                "output_regex",
                "no_output",
                "count",
                "settle_count",
                "visible",
                "visual_text",
                "preview_pixels",
                "preview_integrations",
                "control_value",
                "reset_receipts",
                "clipboard_equals",
                "clipboard_stable",
                "target_visible",
                "absent",
                "config",
                "config_default",
                "same_pids",
                "remaining_pids",
                "file_text",
                "home_json_contains",
                "home_json_equals",
                "git_branch",
                "git_no_branch",
                "symlink_preserved",
                "probe_hex",
                "probe_key_events",
                "probe_mouse_events",
                "geometry_changed",
                "field_value",
                "tab_name",
                "file_exists",
                "agent_field",
                "pane_field",
                "image_paste_result",
                "created_workspace",
                "same_agent_session",
                "remote_workspace_preserved",
                "remote_shell",
                "remote_server_changed",
                "remote_server_stable",
                "remote_file_text",
                "machine_absent",
                "remote_rejected",
                "same_layout",
                "installed_version",
                "upgrade_rolled_back",
                "provider_inputs_unchanged",
            }
            for step in case.steps
        ), case.id


def test_no_unknown_scenario_operations():
    source = ast.parse(Path(__file__).parents[1].joinpath("jam_e2e/steps.py").read_text())
    supported = {
        node.value
        for node in ast.walk(source)
        if isinstance(node, ast.Constant) and isinstance(node.value, str)
    }
    assert {step[0] for case in CASES for step in case.steps} <= supported


def test_unknown_operation_fails():
    with pytest.raises(ValueError, match="Unknown scenario"):
        execute(Mock(), ("not-a-real-operation",), {})


def test_noop_server_restart_cannot_pass_on_an_existing_terminal_label(tmp_path, monkeypatch):
    from jam_e2e import steps

    runtime = tmp_path / "run"
    runtime.mkdir()
    (runtime / "release.json").write_text(json.dumps({
        "pid": os.getpid(), "build": "fixture", "protocol": "fixture",
    }))
    ui = Mock()
    ui.has.return_value = True
    jam = SimpleNamespace(ui=ui, profile=SimpleNamespace(socket=runtime / "jam.sock", repo=tmp_path))
    monkeypatch.setattr(steps, "wait", lambda check, label: wait(check, label, timeout=0))
    with pytest.raises(AssertionError, match="a new live daemon"):
        execute(jam, ("restart_server",), {})
    assert ui.click.call_count == 2
    ui.has.assert_not_called()


def test_wait_times_out_instead_of_succeeding():
    with pytest.raises(AssertionError, match="Timed out"):
        wait(lambda: False, "broken control", timeout=0)


def test_wait_does_not_hide_driver_errors():
    with pytest.raises(ConnectionError):
        wait(Mock(side_effect=ConnectionError("driver lost")), "control", timeout=0)


@pytest.mark.parametrize("text", ["simple", "with'quote", "both'and\"quotes", "日本語"])
def test_xpath_literals(text):
    from lxml import etree

    node = etree.Element("node", name=text)
    assert node.xpath(f"boolean(self::*[@name={literal(text)}])")
