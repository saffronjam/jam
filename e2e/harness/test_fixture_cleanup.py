"""Exercise the real fixture generator through successful and failed cleanup."""

import importlib.util
import sys
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import Mock

import pytest


@pytest.mark.parametrize("failed_restore", [False, True])
def test_all_owned_cleanup_runs_after_a_normal_scenario(tmp_path, monkeypatch, failed_restore):
    source = Path(__file__).parents[1] / "conftest.py"
    spec = importlib.util.spec_from_file_location("fixture_cleanup_contract", source)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    executable = tmp_path / "unused-executable"
    executable.write_text("#!/bin/sh\nexit 1\n")
    executable.chmod(0o700)
    events = []

    def cleanup(name):
        def run():
            events.append(name)
            if failed_restore and name == "input_source":
                raise RuntimeError("Native source restoration failed")
        return run

    def profile(root, *_):
        return SimpleNamespace(root=root, repo=root, env={}, stop=cleanup("profile"),
                               release_checkout=cleanup("checkout"), resume_daemon=cleanup("resume"),
                               release_native_home=cleanup("native_home"))

    ui = Mock(restore_display_scale=cleanup("display"), restore_input_source=cleanup("input_source"),
              restore_clipboard=cleanup("clipboard"), quit=cleanup("ui"))
    client = Mock(close_clients=cleanup("clients"))
    monkeypatch.setattr(module, "Profile", profile)
    monkeypatch.setattr(module, "Desktop", Mock(return_value=ui))
    monkeypatch.setattr(module, "Jam", Mock(return_value=client))
    options = {"--platform": "macos" if sys.platform == "darwin" else "linux",
               "--app": executable, "--cli": executable, "--daemon": executable,
               "--webdriver-url": "unused", "--artifacts-dir": tmp_path / "artifacts"}
    request = SimpleNamespace(config=SimpleNamespace(getoption=options.get), getfixturevalue=Mock())
    generator = module.jam.__wrapped__(request, SimpleNamespace(id="cleanup", fixture=None))
    assert next(generator) is client
    if failed_restore:
        with pytest.raises(pytest.fail.Exception, match="Native source restoration failed"):
            next(generator)
    else:
        with pytest.raises(StopIteration):
            next(generator)
    assert events == ["checkout", "resume", "clients", "display", "input_source",
                      "clipboard", "ui", "profile", "native_home"]
