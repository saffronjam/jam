import ast
from pathlib import Path
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
                "absent",
                "config",
                "config_default",
                "same_pids",
                "file_text",
                "home_json_contains",
                "home_json_equals",
                "git_branch",
                "git_no_branch",
                "symlink_preserved",
                "probe_hex",
                "probe_key_events",
                "probe_mouse_events",
                "field_value",
                "file_exists",
                "agent_field",
                "same_agent_session",
                "provider_inputs_unchanged",
            }
            for step in case.steps
        )


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
