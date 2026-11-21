"""Visibility assertions must reject hidden menus and read live text interfaces."""

from unittest.mock import Mock

from lxml import etree

from jam_e2e.driver import Desktop


def test_hidden_mac_menu_titles_cannot_satisfy_visible_assertions():
    ui = Desktop("macos", "unused", Mock(), Mock())
    ui.session = Mock(
        page_source="""<Application>
      <MenuItem title="Upgrade and Restart" width="0" height="0"/>
      <StaticText value="Currently running" width="100" height="14"/>
      <MenuItem title="Open Release Notes" width="140" height="20"/>
    </Application>"""
    )
    assert ui.has("Currently running")
    assert ui.has("Open Release Notes")
    assert not ui.has("Upgrade and Restart")


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
        b'<XCUIElementTypeStaticText value="Tab 1"/></Application>'
    )
    button, label = Mock(), Mock()
    button.is_displayed.return_value = label.is_displayed.return_value = True
    nodes = dict(zip(root, (button, label)))
    ui.session = Mock()
    ui.session.find_elements.side_effect = lambda _by, query: [
        nodes[node] for node in root.xpath(query)
    ]
    assert ui.element("Tab 1") is button
    root.remove(root[0])
    assert ui.element("Tab 1") is label
