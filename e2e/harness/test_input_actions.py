"""Regression for the action sequence rejected by real Mac2 in TERM-001."""

from unittest.mock import Mock

from selenium.webdriver.remote.webelement import WebElement

from jam_e2e.driver import Desktop


def test_keyboard_input_has_no_pointer_pause_ticks():
    ui = Desktop("macos", "unused", Mock(), Mock())
    ui.session = Mock()
    ui.type("x")
    actions = ui.session.execute.call_args.args[1]["actions"]
    assert len(actions) == 1 and actions[0]["type"] == "key"
    assert actions[0]["actions"] == [
        {"type": "keyDown", "value": "x"},
        {"type": "keyUp", "value": "x"},
    ]
    ui.chord("ctrl+shift+p")
    actions = ui.session.execute.call_args.args[1]["actions"]
    assert len(actions) == 1 and actions[0]["type"] == "key"
    assert [event["type"] for event in actions[0]["actions"]] == ["keyDown"] * 3 + ["keyUp"] * 3


def test_linux_drag_starts_with_a_pointer_move_without_keyboard_ticks():
    ui = Desktop("linux", "unused", Mock(), Mock())
    ui.session = Mock()
    ui.element = lambda name: WebElement(ui.session, name)
    ui.session.execute.return_value = {"value": {"x": 0, "y": 0, "width": 80, "height": 30}}
    ui.drag("source", "destination")
    actions = ui.session.execute.call_args.args[1]["actions"]
    assert len(actions) == 1 and actions[0]["type"] == "pointer"
    assert actions[0]["actions"][0]["type"] == "pointerMove"
    assert actions[0]["actions"][-1]["type"] == "pointerUp"


def test_macos_drag_uses_native_gesture_with_accessible_coordinates():
    ui = Desktop("macos", "unused", Mock(), Mock())
    ui.session = Mock()
    source = Mock(rect={"x": 10, "y": 20, "width": 80, "height": 30})
    target = Mock(rect={"x": 100, "y": 20, "width": 100, "height": 30})
    ui.element = Mock(side_effect=[source, target])
    ui.drag("source", "target", target_x=0.75, target_y=0.25)
    ui.session.execute_script.assert_called_once_with(
        "macos: clickAndDragAndHold",
        {
            "startX": 50,
            "startY": 35,
            "endX": 175,
            "endY": 27.5,
            "duration": 0.5,
            "holdDuration": 0.6,
        },
    )
    ui.session.execute.assert_not_called()
