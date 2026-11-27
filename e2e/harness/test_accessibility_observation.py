"""Visibility assertions must reject hidden menus and read live text interfaces."""

from unittest.mock import Mock, PropertyMock

from lxml import etree
import pytest
from selenium.common.exceptions import InvalidElementStateException

from jam_e2e.driver import Desktop
from jam_e2e.steps import execute


def test_enabled_step_uses_its_deadline_for_appearance_and_readiness(monkeypatch):
    elapsed = 0

    def advance(_seconds):
        nonlocal elapsed
        elapsed += 5

    clock = Mock()
    clock.monotonic.side_effect = lambda: elapsed
    clock.sleep.side_effect = advance
    monkeypatch.setattr("jam_e2e.driver.time", clock)
    ui = Desktop("macos", "unused", Mock(), Mock())
    target = Mock()
    target.is_displayed.return_value = True
    target.is_enabled.side_effect = lambda: elapsed >= 35
    ui.session = Mock()
    ui.session.find_elements.side_effect = lambda *_: [target] if elapsed >= 25 else []
    execute(Mock(ui=ui), ("enabled", "Recover and Reconnect", "button", 90), {})
    assert elapsed == 35
    target.click.assert_not_called()


def test_hidden_mac_menu_titles_cannot_satisfy_visible_assertions():
    ui = Desktop("macos", "unused", Mock(), Mock())
    ui.session = Mock(
        page_source="""<Application>
      <MenuItem title="Upgrade and Restart" width="0" height="0"/>
      <StaticText value="Currently running" width="100" height="14"/>
      <MenuItem title="Open Release Notes" width="140" height="20"/>
      <XCUIElementTypeTouchBar width="685" height="30">
        <XCUIElementTypeButton title="Cancel" width="147" height="32"/>
      </XCUIElementTypeTouchBar>
    </Application>"""
    )
    ui.session.find_elements.return_value = []
    assert ui.has("Currently running")
    assert ui.has("Open Release Notes")
    assert not ui.has("Upgrade and Restart")
    assert not ui.has("Cancel")


def test_atspi_visible_text_is_read_through_the_text_interface():
    ui = Desktop("linux", "unused", Mock(), Mock())
    ui.session = Mock(
        page_source="""<application>
      <text name="Terminal" states="VISIBLE, SHOWING"/>
      <menu_item name="Hidden command" states="VISIBLE"/>
    </application>"""
    )
    visible = Mock(text="actual terminal 日本語")
    visible.is_displayed.return_value = True
    hidden = Mock(text="hidden editor text")
    hidden.is_displayed.return_value = False
    ui.session.find_elements.return_value = [visible, hidden]
    text = ui.texts()
    assert "actual terminal 日本語" in text
    assert "Hidden command" not in text
    assert "hidden editor text" not in text


def test_adjacent_text_does_not_make_a_semantic_control_ambiguous():
    ui = Desktop("macos", "unused", Mock(), Mock())
    root = etree.fromstring(
        b'<Application><XCUIElementTypeButton label="Tab 1"/>'
        b'<XCUIElementTypeStaticText value="Tab 1"/>'
        b'<XCUIElementTypeTouchBar><XCUIElementTypeButton label="Tab 1"/>'
        b'</XCUIElementTypeTouchBar></Application>'
    )
    button, label = Mock(), Mock()
    button.is_displayed.return_value = label.is_displayed.return_value = True
    nodes = dict(zip(root[:2], (button, label)))
    ui.session = Mock()
    ui.session.find_elements.side_effect = lambda _by, query: [
        nodes[node] for node in root.xpath(query)
    ]
    assert ui.element("Tab 1") is button
    root.remove(root[0])
    assert ui.element("Tab 1") is label


def test_machine_selector_distinguishes_workspace_names_and_connection_status():
    ui = Desktop("macos", "unused", Mock(), Mock())
    root = etree.fromstring(b'''<Application>
      <XCUIElementTypeOutline>
        <XCUIElementTypeCell label="Fixture"/>
        <XCUIElementTypeCell label="Fixture"/>
      </XCUIElementTypeOutline>
      <XCUIElementTypeScrollView>
        <XCUIElementTypeActivityIndicator label="Fixture, Retrying soon"/>
      </XCUIElementTypeScrollView>
    </Application>''')
    row = Mock()
    row.is_displayed.return_value = True
    ui.session = Mock(page_source=etree.tostring(root, encoding="unicode"))
    ui.session.find_elements.side_effect = lambda _by, query: [row for _ in root.xpath(query)]
    assert ui.element("Fixture", "machine") is row


def test_modal_close_does_not_select_the_background_pane_close_button():
    ui = Desktop("macos", "unused", Mock(), Mock())
    root = etree.fromstring(b'''<Application><XCUIElementTypeWindow>
      <XCUIElementTypeButton label="Close"/>
      <XCUIElementTypeSheet><XCUIElementTypeButton title="Close"/></XCUIElementTypeSheet>
    </XCUIElementTypeWindow></Application>''')
    background, confirmation = Mock(), Mock()
    background.is_displayed.return_value = confirmation.is_displayed.return_value = True
    nodes = {root[0][0]: background, root[0][1][0]: confirmation}
    ui.session = Mock()
    ui.session.find_elements.side_effect = lambda _by, query: [nodes[node] for node in root.xpath(query)]
    assert ui.element("Close", "button") is confirmation


def test_mac_text_reads_the_full_payload_beyond_the_snapshot_limit():
    ui = Desktop("macos", "unused", Mock(), Mock())
    ui.session = Mock(
        page_source='<Application><XCUIElementTypeTextView value="prefix"/></Application>'
    )
    terminal = Mock(text="prefix" + " " * 600 + "actual updated output")
    terminal.is_displayed.return_value = True
    ui.session.find_elements.return_value = [terminal]
    assert ui.has("actual updated output")


def test_mac_replaced_text_view_is_observed_again_without_replaying_input():
    ui = Desktop("macos", "unused", Mock(), Mock())
    previous, replacement = Mock(), Mock(text="fresh terminal after restart")
    previous.is_displayed.return_value = replacement.is_displayed.return_value = True
    type(previous).text = PropertyMock(side_effect=InvalidElementStateException(
        "Cannot extract the full text of 'TextView' element. Original error: "
        'Code=10008 "No matches found for Descendants matching type TextView"'
    ))
    ui.session = Mock(page_source="<Application/>")
    ui.session.find_elements.side_effect = [[previous], [replacement]]
    ui.expect("fresh terminal after restart")
    assert ui.session.find_elements.call_count == 2
    previous.click.assert_not_called()
    replacement.click.assert_not_called()
    ui.session.execute.assert_not_called()


@pytest.mark.parametrize("message", [
    "The application under test is not running",
    "Cannot extract the full text of 'TextView' element. Code=10008 Multiple matches found",
])
def test_mac_text_observation_does_not_hide_other_invalid_states(message):
    ui = Desktop("macos", "unused", Mock(), Mock())
    terminal = Mock()
    terminal.is_displayed.return_value = True
    type(terminal).text = PropertyMock(side_effect=InvalidElementStateException(message))
    ui.session = Mock(page_source="<Application/>")
    ui.session.find_elements.return_value = [terminal]
    with pytest.raises(InvalidElementStateException, match=message):
        ui.expect("unavailable text")
    ui.session.find_elements.assert_called_once()
