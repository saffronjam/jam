"""Real AppKit input/dialog controls; these do not satisfy Jam product scenarios."""

import json
import os
import plistlib
import shutil
import subprocess
from pathlib import Path
from types import SimpleNamespace

import pytest

from jam_e2e.driver import Desktop, wait


@pytest.fixture
def native_ui(request, tmp_path):
    assert os.uname().sysname == "Darwin", "Run this tool check in the native macOS session"
    app = tmp_path / "NativeProbe.app"
    contents = app / "Contents"
    (contents / "MacOS").mkdir(parents=True)
    (contents / "Info.plist").write_bytes(plistlib.dumps({
        "CFBundleIdentifier": "dev.saffronjam.E2ENativeInputProbe",
        "CFBundleExecutable": "NativeProbe", "CFBundleName": "NativeProbe",
        "CFBundlePackageType": "APPL", "NSHighResolutionCapable": True,
    }))
    source = Path(__file__).parents[1] / "native/macos/input-probe.swift"
    subprocess.run(["/usr/bin/swiftc", str(source), "-o", str(contents / "MacOS/NativeProbe")],
                   check=True, capture_output=True, text=True, timeout=60)
    subprocess.run(["/usr/bin/codesign", "--force", "--sign", "-", str(app)],
                   check=True, capture_output=True, text=True, timeout=15)
    home = tmp_path / "home"
    home.mkdir()
    profile = SimpleNamespace(root=tmp_path, repo=tmp_path, env={
        "HOME": str(home), "PATH": "/usr/bin:/bin:/usr/sbin:/sbin",
        "LANG": "en_US.UTF-8", "JAM_PROBE_EVENTS": str(tmp_path / "native-input.jsonl"),
    })
    ui = Desktop("macos", request.config.getoption("--webdriver-url"), app, profile)
    artifacts = Path(request.config.getoption("--artifacts-dir")) / request.node.name
    artifacts.mkdir(parents=True, exist_ok=True)
    try:
        ui.launch(externally_owned=True)
        yield ui
    finally:
        failures = []
        try:
            if ui.session is not None:
                ui.session.save_screenshot(str(artifacts / "screen.png"))
                if not ui.dialog_active:
                    (artifacts / "ui.xml").write_text(ui.tree())
        except Exception as error:
            (artifacts / "diagnostics-error.txt").write_text(str(error))
        for cleanup in (ui.restore_display_scale, ui.restore_input_source, ui.restore_clipboard, ui.quit):
            try:
                cleanup()
            except Exception as error:
                failures.append(str(error))
        try:
            remaining = ui.application_pids()
            if remaining:
                from jam_e2e.processes import stop_process_tree

                stop_process_tree(remaining)
                failures.append("Native probe processes required forced cleanup")
        except Exception as error:
            failures.append(str(error))
        (artifacts / "observations.json").write_text(json.dumps(ui.observations, indent=2))
        for source in [*tmp_path.glob("native-*.jsonl"), *tmp_path.glob("native-*.log"),
                       *tmp_path.glob("native-*.png"), *tmp_path.glob("scale-*"),
                       *tmp_path.glob("motion-*")]:
            if source.is_file():
                shutil.copy2(source, artifacts / source.name)
        assert not failures, "Native tool cleanup failed: " + "; ".join(failures)


@pytest.mark.parametrize("layout", ["French", "German"])
def test_native_keyboard_layout_in_a_standard_text_field(native_ui, layout):
    ui = native_ui
    ui.input_source(layout)
    ui.click("Probe text", "field")
    expected = "[azerty] {123}: y-z \\"
    ui.type(expected)
    wait(lambda: ui.field_value("Probe text") == expected,
         "exact native punctuation in a standard AppKit text field")


def test_native_reduced_motion_setting_is_observed(native_ui):
    from jam_e2e.preferences import require_reduced_motion

    require_reduced_motion(native_ui)
    native_ui.click("Probe text", "field")
    native_ui.type("native-reduced-motion-input")
    assert native_ui.field_value("Probe text") == "native-reduced-motion-input"


def test_native_query_replacement_uses_one_physical_event_stream(native_ui):
    native_ui.query_burst("Probe text", ("first-query", "second-query", "final-query"))
    assert native_ui.field_value("Probe text") == "final-query"


def test_native_unicode_and_shell_quotes_reach_a_standard_text_field(native_ui):
    ui = native_ui
    receipt = ui.profile.repo / "native-text-input.jsonl"
    samples = ("日本語", "🙂", "stty -echo; PS1='$ '; PS2='> '; PROMPT_COMMAND=''")
    # Each independently checked submission is sent once. A mismatch stops the
    # control immediately; later submissions never retry or repair that input.
    for round_number in range(20):
        for expected in samples:
            with receipt.open("a") as log:
                log.write(json.dumps({"round": round_number, "expected": expected}) + "\n")
            # fill() intentionally pastes non-ASCII field values. Clear only,
            # then exercise the same raw typing path as the terminal case.
            ui.fill("Probe text", "")
            ui.type(expected)
            actual = ui.field_value("Probe text")
            with receipt.open("a") as log:
                log.write(json.dumps({"round": round_number, "actual": actual}) + "\n")
            assert actual == expected, (round_number, expected, actual)


def test_native_menu_item_uses_one_physical_selection(native_ui):
    ui = native_ui
    pid = ui.client_pid()
    ui.click("Open native menu", "button")
    ui.click("Set native marker", "menu")
    ui.element("Native menu selected", "button")
    assert ui.client_pid() == pid
    events = ui.profile.repo / "native-input.jsonl"

    def selections():
        records = [json.loads(line) for line in events.read_text().split("\n")[:-1]]
        return [record for record in records if record.get("type") == "menu_action"]

    wait(lambda: len(selections()) == 1, "the native menu's actual selection callback")
    assert selections() == [
        {"type": "menu_action", "count": 1},
    ]


@pytest.mark.parametrize("commit", [True, False], ids=["commit", "cancel"])
def test_real_japanese_input_method_in_a_standard_text_field(native_ui, commit):
    ui = native_ui
    ui.input_source("Japanese")
    ui.click("Probe text", "field")
    events = ui.profile.repo / "native-input.jsonl"

    def field_state():
        records = [json.loads(line) for line in events.read_text().split("\n")[:-1]] if events.exists() else []
        fields = [event for event in records if event.get("type") == "field"]
        return fields[-1] if fields else {}

    wait(lambda: field_state().get("input_source") == "com.apple.inputmethod.Kotoeri.RomajiTyping.Japanese",
         "the standard field's actual native Japanese input context")
    ui.type("kana")
    if commit:
        ui.key("F6")
    # AX may include marked text in its field value. Read the standard native
    # editor's actual marked state; the product cases separately require zero
    # raw PTY bytes before commit/cancellation.
    wait(lambda: field_state().get("marked") is True, "real AppKit Japanese marked text")
    wait(lambda: field_state().get("editor") == "かな", "native Hiragana preedit")
    ui.session.save_screenshot(str(ui.profile.repo / "native-japanese-preedit.png"))
    if commit:
        ui.key("ENTER")
    else:
        # Standard-field Escape/Clear retained marked Hiragana on this native
        # source. Erase the two composing characters with real Backspaces;
        # require the intermediate preedit as well as empty/unmarked recovery.
        # Jam's separate single-Escape/zero-PTY case remains unchanged.
        ui.key("BACKSPACE")
        wait(lambda: field_state().get("editor") == "か" and field_state().get("marked") is True,
             "physical deletion edits the existing Japanese preedit")
        ui.key("BACKSPACE")
    wait(lambda: ui.field_value("Probe text") == ("かな" if commit else ""),
         "native Japanese commit/cancellation")
    wait(lambda: field_state().get("marked") is False, "native Japanese marked text ends")
    ui.restore_input_source()
    ui.click("Probe text", "field")
    ui.type("x")
    assert ui.field_value("Probe text") == ("かなx" if commit else "x")


@pytest.mark.parametrize("alias", [False, True], ids=["canonical", "symlink"])
def test_native_accessory_process_can_return_to_the_same_main_app(native_ui, alias):
    ui = native_ui
    if alias:
        link = ui.profile.root / "bundle-alias"
        link.symlink_to(ui.app.parent, target_is_directory=True)
        ui.app = link / ui.app.name
    main = ui.client_pid()
    ui.click("Open accessory prompt", "button")
    ui.dialog("Native accessory prompt")
    helper = ui.dialog_pid
    ui.dialog_button("Cancel")
    ui.finish_dialogs()
    assert ui.client_pid() == main and helper != main
    ui.click("Probe text", "field")
    ui.type("after-native-prompt")
    assert ui.field_value("Probe text") == "after-native-prompt"
    events = ui.profile.repo / "native-input.jsonl"
    wait(lambda: any(event.get("helper_pid") == helper and event.get("helper_status") == 1
                     for event in map(json.loads, events.read_text().splitlines())),
         "owned native helper reports the actual Cancel result")


def test_explicit_attachment_rejects_a_terminated_main_process(native_ui):
    from selenium.common.exceptions import WebDriverException

    ui = native_ui
    pid = ui.client_pid()
    ui.attach_process(pid)
    ui.element("Probe text", "field")
    helper = Path(__file__).parents[1] / "native/macos/terminate.swift"
    subprocess.run(["/usr/bin/swift", str(helper), str(ui.app), str(pid)],
                   check=True, capture_output=True, text=True, timeout=30)
    wait(lambda: not ui.application_pids(), "the explicitly attached process really exits")
    with pytest.raises(WebDriverException, match="application under test is not running"):
        ui.tree()
    assert not ui.application_pids(), "Reading a terminated app must not relaunch it"


def test_native_retina_output_and_input(native_ui):
    ui = native_ui
    ui.higher_display_scale(surface=("Probe text", "field", None))
    assert ui.display_scale_state.observe()["scale"] == 2
    ui.click("Probe text", "field")
    ui.type("native-retina-input")
    assert ui.field_value("Probe text") == "native-retina-input"
    ui.resize(700, 400)
    ui.restore_display_scale()
    assert ui.display_scale_state.observe()["scale"] == 1


def test_native_pasteboard_round_trips_images_and_text(native_ui):
    from jam_e2e.clipboard import image_fixture, representations

    ui = native_ui
    before = ui._mac_clipboard("capture")
    for kind in ("png", "tiff"):
        image, _ = image_fixture(kind)
        ui.clipboard_image(kind, image, "AppKit clipboard 日本語")
        [actual] = ui._mac_clipboard("capture")
        [expected] = representations("macos", kind, image, "AppKit clipboard 日本語")
        assert {value["type"]: value["data"] for value in actual} == {
            value["type"]: value["data"] for value in expected
        }
    ui.click("Probe text", "field")
    ui.chord("cmd+v")
    wait(lambda: ui.field_value("Probe text") == "AppKit clipboard 日本語",
         "standard AppKit field reads the text flavor")
    ui.restore_clipboard()
    assert ui._mac_clipboard("capture") == before
