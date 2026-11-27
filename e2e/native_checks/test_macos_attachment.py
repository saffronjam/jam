"""Native tool compatibility proof, separate from the shared product contract."""

from types import SimpleNamespace

import pytest


@pytest.fixture
def case():
    return SimpleNamespace(id="DRIVER-PROCESS", fixture=None)


def test_observed_process_attachment_preserves_the_running_app(jam):
    assert jam.ui.platform == "macos", "This tool check requires a native macOS runner"
    pid = jam.ui.client_pid()
    jam.ui.attach_process(pid)
    assert jam.ui.client_pid() == pid
    jam.ui.element("Terminal", "terminal", index=0)
    jam.run("printf exact-native-process")
    assert any("exact-native-process" in jam.profile.capture(pane["id"])
               for pane in jam.profile.layout()["panes"])
    jam.ui.quit()
    assert not jam.ui.application_pids(), "Explicit attachment must not leave a client after quit"
