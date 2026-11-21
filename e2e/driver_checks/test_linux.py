"""Low-level Linux driver proof against GTK widgets, separate from Jam acceptance."""

import os
import re
from pathlib import Path
from types import SimpleNamespace

from jam_e2e.driver import Desktop, wait


def test_native_wayland_driver(request, tmp_path):
    assert os.environ.get("KWIN_PID"), "Run inside the isolated KWin Wayland wrapper"
    launcher = tmp_path / "gtk-probe"
    probe = Path(__file__).with_name("gtk_probe.py")
    launcher.write_text(f'#!/bin/sh\nexec /usr/bin/python3 "{probe}"\n')
    launcher.chmod(0o700)
    # The driver proof owns no Jam daemon, state or product objects.
    profile = SimpleNamespace(repo=tmp_path, env=dict(os.environ))
    ui = Desktop("linux", request.config.getoption("--webdriver-url"), launcher, profile)
    artifacts = Path(request.config.getoption("--artifacts-dir")) / "linux-driver-proof"
    artifacts.mkdir(parents=True, exist_ok=True)
    try:
        ui.launch()
        ui.click("Increment", "button")
        ui.expect("Count: 1")
        ui.expect("Slider: 50")
        ui.drag("Probe slider", "Probe slider", target_x=0.85)

        def dragged():
            values = re.findall(r"Slider:\s*(\d+)", ui.texts())
            return values and 70 <= int(values[-1]) <= 95

        wait(dragged, "real Wayland pointer drag changes a GTK slider")
        ui.fill("Probe text", "日本語 café")
        wait(lambda: ui.field_value("Probe text") == "日本語 café", "Unicode field input")
        ui.chord("ctrl+a")
        ui.type("Replacement")
        assert ui.field_value("Probe text") == "Replacement"
        ui.clipboard("native clipboard 日本語")
        assert ui.clipboard() == "native clipboard 日本語"
        assert ui.session.save_screenshot(str(artifacts / "gtk.png"))
    finally:
        try:
            if ui.session:
                (artifacts / "ui.xml").write_text(ui.tree())
        except Exception as error:  # noqa: BLE001 - diagnostics preserve the primary failure
            (artifacts / "diagnostics-error.txt").write_text(str(error))
        try:
            if ui.session:
                ui.session.save_screenshot(str(artifacts / "gtk.png"))
        except Exception as error:  # noqa: BLE001 - screenshot failure is diagnostic
            (artifacts / "screenshot-error.txt").write_text(str(error))
        try:
            ui.restore_clipboard()
        finally:
            ui.quit()
