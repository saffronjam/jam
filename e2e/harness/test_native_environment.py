"""An input-source change must remain recoverable when its reply is lost."""

import json
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import Mock

import pytest

from jam_e2e.driver import Desktop


def test_failed_layout_change_still_restores_the_previous_native_layout():
    ui = Desktop("linux", "unused", Mock(), Mock())
    ui.activate = Mock()
    current = "us"

    def call(command, request):
        nonlocal current
        assert command == "linux: keyboardLayout"
        before = current
        if not request.get("inspect"):
            current = request["layout"]
            if current == "fr":
                raise ConnectionError("Native layout changed before the reply was lost")
        return {"before": before, "selected": request["layout"]}

    ui.session = Mock()
    ui.session.execute_script.side_effect = call
    with pytest.raises(ConnectionError, match="reply was lost"):
        ui.input_source("French")
    assert current == "fr"
    ui.restore_input_source()
    assert current == "us"
    assert ui.input_sources == []


@pytest.mark.parametrize("failed", [False, True])
def test_raw_unicode_preserves_native_source_and_does_not_retry(tmp_path, failed):
    ui = Desktop("macos", "unused", Path("/owned/Jam.app"), SimpleNamespace(repo=tmp_path))
    ui.activate_owned_client = Mock()
    ui.client_pid = Mock(return_value=123)
    ui.input_source = Mock(side_effect=AssertionError("Unicode must not change native preferences"))
    previous = {"before": "com.apple.keylayout.US", "restore_disable": []}
    ui.input_sources.append(previous)
    commands = []

    def native(command):
        commands.append(command)
        assert Path(command[1]).name == "unicode-keys.swift"
        assert command[2:] == ["/owned/Jam.app", "123", "日本語"]
        if failed:
            raise ConnectionError("Native event delivery failed")
        return SimpleNamespace(stdout=json.dumps({"source": "com.apple.keylayout.French", "key_events": 6}))

    ui.native_input_command = native
    if failed:
        with pytest.raises(ConnectionError, match="Native event delivery failed"):
            ui.type("日本語")
    else:
        ui.type("日本語")
        assert ui.observations[-1]["source"] == "com.apple.keylayout.French"
    assert ui.input_sources == [previous]
    ui.input_source.assert_not_called()
    assert len(commands) == 1
