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


def test_native_dialog_keys_use_the_explicit_desktop_stream():
    ui = Desktop("macos", "unused", Mock(), Mock())
    ui.session = Mock()
    ui.dialog_active = True
    ui.type("fixture-password")
    actions = ui.session.execute.call_args.args[1]["actions"]
    assert len(actions) == 1 and actions[0]["type"] == "key"
    assert actions[0]["id"] == "desktop-keyboard"
    ui.key("ESCAPE")
    actions = ui.session.execute.call_args.args[1]["actions"]
    assert actions[0]["id"] == "desktop-keyboard"


def test_layout_punctuation_releases_option_and_shift_before_ordinary_input():
    ui = Desktop("macos", "unused", Mock(), Mock())
    ui.session = Mock()
    ui.layout_keys = {"[": {"key": "(", "shift": True}}
    ui.type("[x")
    events = ui.session.execute.call_args.args[1]["actions"][0]["actions"]
    assert [(event["type"], event["value"]) for event in events] == [
        ("keyDown", "\ue00a"), ("keyDown", "\ue008"),
        ("keyDown", "("), ("keyUp", "("),
        ("keyUp", "\ue008"), ("keyUp", "\ue00a"),
        ("keyDown", "x"), ("keyUp", "x"),
    ]


def test_linux_drag_starts_with_a_pointer_move_without_keyboard_ticks():
    ui = Desktop("linux", "unused", Mock(), Mock())
    ui.session = Mock()
    ui.element = lambda name: WebElement(ui.session, name)
    ui.session.execute.return_value = {"value": {"x": 0, "y": 0, "width": 80, "height": 30}}
    ui.drag("source", "destination")
    actions = ui.session.execute.call_args.args[1]["actions"]
    assert len(actions) == 1 and actions[0]["type"] == "pointer"
    assert actions[0]["actions"][0]["type"] == "pointerMove"
    assert actions[0]["actions"][0]["origin"] == "viewport"
    assert (actions[0]["actions"][0]["x"], actions[0]["actions"][0]["y"]) == (40, 15)
    assert actions[0]["actions"][-1]["type"] == "pointerUp"


def test_linux_click_injects_pointer_input_at_accessible_center():
    ui = Desktop("linux", "unused", Mock(), Mock())
    ui.session = Mock()
    target = Mock(rect={"x": 10, "y": 20, "width": 80, "height": 30})
    ui.element = Mock(return_value=target)
    ui.click("Increment", "button")
    target.click.assert_not_called()
    actions = ui.session.execute.call_args.args[1]["actions"]
    assert len(actions) == 1 and actions[0]["type"] == "pointer"
    move, down, up = actions[0]["actions"]
    assert (move["origin"], move["x"], move["y"]) == ("viewport", 50, 35)
    assert down["type"] == "pointerDown" and up["type"] == "pointerUp"


def test_observed_macos_process_uses_desktop_input_after_reading_element_bounds():
    ui = Desktop("macos", "unused", Mock(), Mock())
    ui.session = Mock()
    ui.observed_process = 12345
    ui.activate_owned_client = Mock()
    target = Mock(rect={"x": 10, "y": 20, "width": 80, "height": 30})
    ui.element = Mock(return_value=target)
    ui.click("Probe text", "field")
    ui.type("after-native-prompt")
    target.click.assert_not_called()
    assert ui.activate_owned_client.call_count == 2
    assert all(call.kwargs == {"inspect": True}
               for call in ui.activate_owned_client.call_args_list)
    calls = ui.session.execute.call_args_list
    pointer = calls[0].args[1]["actions"][0]
    assert pointer["id"] == "desktop-pointer"
    assert pointer["actions"][0]["origin"] == "viewport"
    assert (pointer["actions"][0]["x"], pointer["actions"][0]["y"]) == (50, 35)
    assert calls[1].args[1]["actions"][0]["id"] == "desktop-keyboard"


def test_macos_menu_uses_enabled_target_bounds_and_one_desktop_click():
    ui = Desktop("macos", "unused", Mock(), Mock())
    ui.session = Mock()
    ui.activate_owned_client = Mock()
    target = Mock(rect={"x": 110, "y": 588, "width": 140, "height": 22})
    target.is_enabled.return_value = True
    ui.element = Mock(return_value=target)
    ui.click("Reconnect", "menu")
    target.click.assert_not_called()
    target.is_enabled.assert_called_once_with()
    ui.activate_owned_client.assert_called_once_with(inspect=True)
    sources = ui.session.execute.call_args.args[1]["actions"]
    assert len(sources) == 1 and sources[0]["id"] == "desktop-pointer"
    move, down, up = sources[0]["actions"]
    assert (move["origin"], move["x"], move["y"]) == ("viewport", 180, 599)
    assert down["type"] == "pointerDown" and up["type"] == "pointerUp"


def test_native_terminal_focus_does_not_require_an_enabled_command_control():
    ui = Desktop("macos", "unused", Mock(), Mock())
    ui.session = Mock()
    terminal = Mock()
    terminal.is_enabled.return_value = False
    ui.element = Mock(return_value=terminal)
    ui.click("Terminal", "terminal", index=0)
    terminal.click.assert_called_once_with()
    terminal.is_enabled.assert_not_called()


def test_empty_field_input_deletes_the_selection_without_an_empty_action_request():
    ui = Desktop("macos", "unused", Mock(), Mock())
    ui.session = Mock(page_source="<Application/>")
    ui.element = Mock(return_value=Mock())
    ui.fill("Shortcut", "")
    calls = ui.session.execute.call_args_list
    assert len(calls) == 2
    last = calls[-1].args[1]["actions"][0]["actions"]
    assert last == [
        {"type": "keyDown", "value": "\ue003"},
        {"type": "keyUp", "value": "\ue003"},
    ]


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


def test_secondary_display_drag_has_explicit_viewport_coordinates_and_no_pause_items():
    ui = Desktop("macos", "unused", Mock(), Mock())
    ui.session = Mock()
    ui.display_origin = {"x": 1920, "y": 0}
    ui.window_rect = Mock(return_value={"x": 1940, "y": 40, "width": 600, "height": 400})
    ui._window_drag(2240, 52, 2340, 52)
    actions = ui.session.execute.call_args.args[1]["actions"]
    assert len(actions) == 1 and actions[0]["id"] == "desktop-pointer"
    events = actions[0]["actions"]
    assert [event["type"] for event in events] == ["pointerMove", "pointerDown", "pointerMove", "pointerUp"]
    assert events[0]["origin"] == events[2]["origin"] == "viewport"
    assert (events[0]["x"], events[2]["x"]) == (2240, 2340)
