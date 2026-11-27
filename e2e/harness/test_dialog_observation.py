"""A native alert's first animation frame must not freeze the OCR crop."""

import io
from types import SimpleNamespace
from unittest.mock import Mock

import pytest
from PIL import Image

from jam_e2e import dialogs
from jam_e2e.driver import Desktop


def observe_two_frames(check, description):
    for _ in range(2):
        value = check()
        if value:
            return value
    raise AssertionError(description)


def test_native_dialog_title_follows_the_owned_window_as_it_expands(tmp_path, monkeypatch):
    ui = Desktop("macos", "unused", Mock(), SimpleNamespace(repo=tmp_path))
    ui.session = Mock()
    png = io.BytesIO()
    Image.new("RGB", (1024, 768), "white").save(png, format="PNG")
    ui.session.get_screenshot_as_png.return_value = png.getvalue()
    ui.session.execute_script.return_value = {
        "display": {"rect": {"x": 0, "y": 0, "width": 1024, "height": 768}},
    }
    tiny = {"pid": 456, "rect": {"x": 502, "y": 251, "width": 20, "height": 12}}
    full = {"pid": 456, "rect": {"x": 326, "y": 135, "width": 372, "height": 244}}
    ui.dialog_window = Mock(side_effect=[tiny, tiny, full])
    ui.application_pids = Mock(return_value={123, 456})
    monkeypatch.setattr(dialogs, "wait", observe_two_frames)
    monkeypatch.setattr(dialogs.shutil, "which", lambda name: "/fixture/tesseract")

    def ocr(command, **kwargs):
        with Image.open(command[1]) as image:
            expanded = image.size == (744, 488)
        header = "level\tblock_num\tpar_num\tline_num\tleft\ttop\twidth\theight\ttext\n"
        words = ("5\t1\t1\t1\t40\t40\t25\t12\tSSH\n"
                 "5\t1\t1\t1\t70\t40\t100\t12\tAuthentication\n") if expanded else ""
        return SimpleNamespace(stdout=header + words)

    monkeypatch.setattr(dialogs.subprocess, "run", ocr)
    ui.dialog("SSH Authentication")

    assert ui.dialog_pid == 456 and ui.dialog_primary == {123}
    assert ui.session.get_screenshot_as_png.call_count == 2
    assert ui.dialog_window.call_count == 3
    with Image.open(tmp_path / "dialog-window.png") as crop:
        assert crop.size == (372, 244)
    ui.session.execute.assert_not_called()  # Observation never repeats an input action.


def test_native_dialog_observation_rejects_a_replacement_prompt(tmp_path, monkeypatch):
    ui = Desktop("macos", "unused", Mock(), SimpleNamespace(repo=tmp_path))
    ui.session = Mock()
    bounds = {"x": 326, "y": 135, "width": 372, "height": 244}
    ui.dialog_window = Mock(side_effect=[
        {"pid": 456, "rect": bounds}, {"pid": 789, "rect": bounds},
    ])
    monkeypatch.setattr(dialogs.shutil, "which", lambda name: "/fixture/tesseract")
    with pytest.raises(AssertionError, match="prompt changed while observing"):
        ui.dialog("SSH Authentication")
    ui.session.get_screenshot_as_png.assert_not_called()
    ui.session.execute.assert_not_called()
