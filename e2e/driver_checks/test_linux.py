"""Low-level Linux driver proof against GTK widgets, separate from Jam acceptance."""

import json
import os
import re
import shutil
from pathlib import Path
from types import SimpleNamespace

from selenium.webdriver.common.actions.action_builder import ActionBuilder

from jam_e2e.driver import Desktop, wait
from jam_e2e.clients import Clients
from jam_e2e.dialogs import screen_target


def test_native_wayland_driver(request, tmp_path):
    assert os.environ.get("KWIN_PID"), "Run inside the isolated KWin Wayland wrapper"
    launcher = tmp_path / "gtk-probe"
    probe = Path(__file__).with_name("gtk_probe.py")
    launcher.write_text(f'#!/bin/sh\nexec /usr/bin/python3 "{probe}"\n')
    launcher.chmod(0o700)
    # The driver proof owns no Jam daemon, state or product objects.
    # Retain compositor-level input evidence while checking the GTK adapter.
    from fixtures.native_preferences import reduced_motion
    from jam_e2e.preferences import require_reduced_motion

    profile = SimpleNamespace(root=tmp_path, repo=tmp_path, env=dict(
        os.environ, WAYLAND_DEBUG="client", XDG_CONFIG_HOME=str(tmp_path / "config")))
    reduced_motion(tmp_path, profile.env)
    ui = Desktop("linux", request.config.getoption("--webdriver-url"), launcher, profile)
    artifacts = Path(request.config.getoption("--artifacts-dir")) / "linux-driver-proof"
    artifacts.mkdir(parents=True, exist_ok=True)
    clients = None
    try:
        ui.launch()
        require_reduced_motion(ui)
        assert ui.element("Increment", "button").is_enabled(), "Enabled GTK action must be reported enabled"
        assert not ui.element("Disabled action", "button").is_enabled(), "Insensitive GTK action must stay disabled"
        (artifacts / "bounds.json").write_text(
            json.dumps(
                {
                    "button": ui.element("Increment", "button").rect,
                    "slider": ui.element("Probe slider").rect,
                },
                indent=2,
            )
        )
        ui.click("Increment", "button")
        ui.expect("Count: 1")
        ui.query_burst("Probe text", ("first-query", "second-query", "final-query"))
        assert ui.field_value("Probe text") == "final-query"
        ui.expect("Slider: 50")
        ui.drag("Probe slider", "Probe slider", target_x=0.85)

        def dragged():
            values = re.findall(r"Slider:\s*(\d+)", ui.texts())
            return values and 70 <= int(values[-1]) <= 95

        wait(dragged, "real Wayland pointer drag changes a GTK slider")
        ui.fill("Probe text", "日本語 café")
        (artifacts / "unicode-input.json").write_text(json.dumps({
            "observed": ui.field_value("Probe text"),
            "expected": "日本語 café",
        }, ensure_ascii=False, indent=2))
        wait(lambda: ui.field_value("Probe text") == "日本語 café", "Unicode field input")
        ui.chord("ctrl+a")
        ui.type("Replacement")
        assert ui.field_value("Probe text") == "Replacement"
        ui.fill("Probe text", "")
        ui.begin_composition()
        assert ui.field_value("Probe text") == "", "Preedit must remain uncommitted"
        assert ui.session.save_screenshot(str(artifacts / "gtk-preedit.png"))
        ui.finish_composition(True)
        wait(lambda: ui.field_value("Probe text") == "é", "native composition commit")
        ui.fill("Probe text", "")
        ui.begin_composition()
        ui.finish_composition(False)
        assert ui.field_value("Probe text") == "", "Cancelled preedit must not become text"
        ui.type("after-compose")
        wait(lambda: ui.field_value("Probe text") == "after-compose", "typing after composition")
        ui.resize(900, 580)
        ui.click("Increment", "button")
        ui.expect("Count: 2")
        assert ui.field_value("Probe text") == "after-compose"
        ui.fill("Bottom field", "scrolled-entry")
        wait(lambda: ui.field_value("Bottom field") == "scrolled-entry", "physical scroll and field edit")
        ui.clipboard("native clipboard 日本語")
        assert ui.clipboard() == "native clipboard 日本語"
        assert ui.session.save_screenshot(str(artifacts / "gtk.png"))
        clients = Clients(ui)
        peer = clients.open("second")
        peer.click("Increment", "button")
        peer.expect("Count: 1")
        peer.fill("Probe text", "second process")
        primary = clients.select("primary")
        primary.expect("Count: 2")
        assert primary.field_value("Probe text") == "after-compose"
        peer = clients.select("second")
        assert peer.field_value("Probe text") == "second process"
        (artifacts / "client-pids.json").write_text(json.dumps(clients.pids, indent=2))
        clients.close("second")
        ui.click("Increment", "button")
        ui.expect("Count: 3")
        ui.fill("Probe text", "selectionword")
        point = screen_target(ui, "selectionword", prefix="selection")
        actions = ActionBuilder(ui.session)
        actions.pointer_action.move_to_location(round(point["x"]), round(point["y"]))
        actions.pointer_action.click().pause(0.1).click()
        actions.perform()
        # GtkEntry uses the ordinary Ctrl+C desktop shortcut. Jam's terminal
        # has its own Ctrl+Shift+C action, tested by the shared SELECT cases.
        ui.chord("ctrl+c")
        wait(lambda: ui.clipboard() == "selectionword", "physical double click and clipboard copy")
        for layout in ("French", "German"):
            ui.input_source(layout)
            ui.fill("Probe text", "[azerty] {123}: y-z")
            assert ui.field_value("Probe text") == "[azerty] {123}: y-z"
        ui.restore_input_source()
        ui.higher_display_scale(surface=("Probe text", "field", None))
        ui.click("Increment", "button")
        ui.expect("Count: 4")
        ui.fill("Probe text", "scaled-native-input")
        assert ui.field_value("Probe text") == "scaled-native-input"
        ui.resize(820, 540)
        assert ui.session.save_screenshot(str(artifacts / "gtk-fractional-scale.png"))
        ui.restore_display_scale()
        ui.click("Increment", "button")
        ui.expect("Count: 5")
        for source in tmp_path.glob("selection-*"):
            shutil.copy2(source, artifacts / source.name)
    finally:
        motion = tmp_path / "motion-observation.json"
        if motion.is_file():
            shutil.copy2(motion, artifacts / motion.name)
        for source in tmp_path.glob("scale-*"):
            if source.is_file():
                shutil.copy2(source, artifacts / source.name)
        try:
            if ui.session:
                (artifacts / "ui.xml").write_text(ui.tree())
                (artifacts / "visible.txt").write_text(ui.texts())
                (artifacts / "window-rect.json").write_text(json.dumps(ui.window_rect(), indent=2))
                (artifacts / "driver-observations.json").write_text(json.dumps(ui.observations, indent=2))
        except Exception as error:  # noqa: BLE001 - diagnostics preserve the primary failure
            (artifacts / "diagnostics-error.txt").write_text(str(error))
        try:
            if ui.session:
                ui.session.save_screenshot(str(artifacts / "gtk.png"))
        except Exception as error:  # noqa: BLE001 - screenshot failure is diagnostic
            (artifacts / "screenshot-error.txt").write_text(str(error))
        try:
            if clients is not None:
                clients.close_all()
        finally:
            try:
                ui.restore_display_scale()
            finally:
                try:
                    ui.restore_input_source()
                finally:
                    try:
                        ui.restore_clipboard()
                    finally:
                        ui.quit()
