"""Regressions from real macOS terminal captures, without altering their text."""

from unittest.mock import Mock

import pytest

from jam_e2e.steps import execute


def test_working_directory_observation_handles_symlinks_and_soft_wraps(tmp_path):
    physical = tmp_path / "physical" / "Project ü with spaces"
    physical.mkdir(parents=True)
    alias = tmp_path / "alias"
    alias.symlink_to(physical.parent, target_is_directory=True)
    jam = Mock()
    jam.profile.repo = alias / physical.name
    jam.profile.layout.return_value = {"panes": [{"id": "pane"}]}
    path = str(physical)
    jam.profile.capture.return_value = "$ " + path[:70] + "\n" + path[70:] + "\n$ "
    execute(jam, ("output_repo",), {})


@pytest.mark.parametrize("actual", ["other", "Project ü withspaces"])
def test_wrong_working_directory_still_fails(tmp_path, monkeypatch, actual):
    from jam_e2e import steps
    from jam_e2e.driver import wait

    monkeypatch.setattr(steps, "wait", lambda check, description: wait(check, description, timeout=0))
    jam = Mock()
    jam.profile.repo = tmp_path / "Project ü with spaces"
    jam.profile.layout.return_value = {"panes": [{"id": "pane"}]}
    jam.profile.capture.return_value = str(tmp_path / actual)
    with pytest.raises(AssertionError, match="terminal output_repo"):
        execute(jam, ("output_repo",), {})
