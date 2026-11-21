"""Low-level Linux driver proof against GTK widgets, separate from Jam acceptance."""

import os
from pathlib import Path
from types import SimpleNamespace

from jam_e2e.driver import Desktop


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
        ui.fill("Probe text", "日本語 café")
        ui.chord("ctrl+a")
        ui.type("Replacement")
        assert ui.field_value("Probe text") == "Replacement"
        ui.clipboard("native clipboard 日本語")
        assert ui.clipboard() == "native clipboard 日本語"
        assert ui.session.save_screenshot(str(artifacts / "gtk.png"))
    finally:
        if ui.session:
            (artifacts / "ui.xml").write_text(ui.tree())
        ui.restore_clipboard()
        ui.quit()
