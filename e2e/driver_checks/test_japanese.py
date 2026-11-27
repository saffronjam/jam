"""Prove real IBus/Mozc GTK preedit; no Jam application is used here."""

import json
import os
import shutil
import sys
from pathlib import Path
from types import SimpleNamespace

import pytest

from fixtures.build_bundle import recipes
from jam_e2e.bundle import prepare
from jam_e2e.driver import Desktop, wait


@pytest.mark.parametrize("commit", [True, False], ids=["commit", "cancel"])
def test_real_japanese_input_in_a_native_gtk_field(request, tmp_path, commit):
    assert os.environ.get("KWIN_PID"), "Use the isolated native Wayland runner"
    home = tmp_path / "home"
    home.mkdir()
    fixtures = tmp_path / "bundle"
    fixtures.mkdir()
    for name in ("native_ime_service.py", "ibus_engine.py"):
        shutil.copy2(Path(__file__).parents[1] / "fixtures" / name, fixtures)
    manifest = fixtures / "manifest.json"
    manifest.write_text(json.dumps({"schema": 1, "recipes": {
        "native-japanese-input": recipes()["recipes"]["native-japanese-input"],
    }}))
    launcher = tmp_path / "gtk-probe"
    launcher.write_text(f'#!/bin/sh\nexec /usr/bin/python3 "{Path(__file__).with_name("gtk_probe.py")}"\n')
    launcher.chmod(0o700)
    profile = SimpleNamespace(root=tmp_path, home=home, repo=tmp_path,
                              config=tmp_path / "config/jam/config.toml", socket=tmp_path / "socket",
                              cli=Path(sys.executable), daemon=Path(sys.executable), env=dict(
                                  os.environ, HOME=str(home), XDG_CONFIG_HOME=str(tmp_path / "config"),
                                  XDG_DATA_HOME=str(tmp_path / "data")))
    ui = Desktop("linux", request.config.getoption("--webdriver-url"), launcher, profile)
    artifacts = Path(request.config.getoption("--artifacts-dir")) / request.node.name
    artifacts.mkdir(parents=True, exist_ok=True)
    resources = None
    try:
        resources = prepare(manifest, "native-japanese-input", profile)
        ui.launch()
        ui.click("Probe text", "field")
        ui.input_source("Japanese")
        ui.type("kana")
        if commit:
            ui.key("F6")
        assert ui.field_value("Probe text") == "", "Native Japanese preedit was committed early"
        ui.session.save_screenshot(str(artifacts / "native-japanese-preedit.png"))
        ui.key("ENTER" if commit else "ESCAPE")
        wait(lambda: ui.field_value("Probe text") == ("かな" if commit else ""),
             "native Mozc commit/cancellation")
        ui.restore_input_source()
        ui.type("x")
        assert ui.field_value("Probe text") == ("かなx" if commit else "x")
    finally:
        errors = []
        if ui.session is not None:
            try:
                (artifacts / "ui.xml").write_text(ui.tree())
                ui.session.save_screenshot(str(artifacts / "screen.png"))
            except Exception as error:
                (artifacts / "diagnostics-error.txt").write_text(str(error))
        for cleanup in [ui.restore_input_source, ui.quit, *([resources.stop] if resources else [])]:
            try:
                cleanup()
            except Exception as error:
                errors.append(str(error))
        (artifacts / "observations.json").write_text(json.dumps(ui.observations, indent=2))
        for source in [*tmp_path.glob("fixture-*"), tmp_path / "ime.ready"]:
            if source.is_file():
                shutil.copy2(source, artifacts / source.name)
        assert not errors, "Owned native IME cleanup failed: " + "; ".join(errors)
