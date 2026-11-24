"""Native drivers share WebDriver operations, not product decision logic."""

import json
import os
import plistlib
import shlex
import socket
import subprocess
import time
import unicodedata
from itertools import groupby
from pathlib import Path
from xml.etree import ElementTree

from appium import webdriver
from appium.options.common import AppiumOptions
from appium.webdriver.client_config import AppiumClientConfig
from selenium.common.exceptions import (
    InvalidElementStateException,
    NoSuchElementException,
    StaleElementReferenceException,
)
from selenium.webdriver.common.actions.action_builder import ActionBuilder
from selenium.webdriver.common.actions.key_input import KeyInput
from selenium.webdriver.common.actions.pointer_input import PointerInput
from selenium.webdriver.common.by import By
from selenium.webdriver.common.keys import Keys


def literal(text):
    if "'" not in text:
        return "'" + text + "'"
    if '"' not in text:
        return '"' + text + '"'
    return "concat(" + ', "\'", '.join("'" + part + "'" for part in text.split("'")) + ")"


def wait(check, description, timeout=20):
    deadline = time.monotonic() + timeout
    while True:
        try:
            value = check()
            if value:
                return value
        except (NoSuchElementException, StaleElementReferenceException):
            pass
        if time.monotonic() >= deadline:
            raise AssertionError(f"Timed out waiting for {description}")
        time.sleep(0.1)


class Desktop:
    def __init__(self, platform, endpoint, app, profile):
        self.platform, self.endpoint, self.app, self.profile = platform, endpoint, app, profile
        self.session = None
        self.saved_clipboard = None
        self.saved_binary_clipboard = None
        self.clipboard_owner = self
        self.dialog_active = False
        self.composition = None
        self.observations = []
        self.input_sources = []
        self.layout_keys = {}
        self.native_input_method = None
        self.capture_scale = 1
        self.display_origin = None
        self.display_scale_state = None
        self.external_app = False
        self.wda_port = None
        self.observed_process = None

    def launch(self, directory=None, externally_owned=False, attach_existing=False):
        self.observed_process = None
        options = AppiumOptions()
        arguments = ["--directory", str(directory or self.profile.repo)]
        if self.platform == "macos":
            self.external_app = self.external_app or externally_owned
            if self.external_app:
                assert self.app.resolve().is_relative_to(self.profile.root.resolve()), \
                    "External application ownership requires its private installation"
            if attach_existing:
                assert self.external_app, "Automation reconnection must preserve the application"
            with (self.app / "Contents/Info.plist").open("rb") as info:
                bundle_id = plistlib.load(info)["CFBundleIdentifier"]
            caps = {
                "platformName": "mac",
                "appium:automationName": "mac2",
                "appium:appPath": str(self.app),
                "appium:bundleId": bundle_id,
                "appium:arguments": arguments,
                "appium:environment": self.profile.env,
                # Installer/rollback observations can take 90 seconds without
                # a WebDriver command. Keep the native session alive through it.
                "appium:newCommandTimeout": 180,
            }
            if self.external_app:
                caps["appium:skipAppKill"] = True
            if attach_existing:
                caps["appium:noReset"] = True
                # Start only the automation runner. Mac2 noReset with an app
                # path can launch a stopped app; explicit observed-PID binding
                # below must never repair a product crash by relaunching it.
                for key in ("appium:appPath", "appium:bundleId", "appium:arguments", "appium:environment"):
                    caps.pop(key)
            if self.wda_port is not None:
                caps["appium:systemPort"] = self.wda_port
        else:
            if not self.profile.env.get("KWIN_PID"):
                raise AssertionError(
                    "Linux acceptance requires the isolated KWin Wayland runner (KWIN_PID)"
                )
            caps = {
                "platformName": "linux",
                "appium:app": shlex.join([str(self.app), *arguments]),
                "appium:environ": dict(self.profile.env, GDK_BACKEND="wayland"),
                "timeouts": {"implicit": 15000},
            }
        options.load_capabilities(caps)
        config = AppiumClientConfig(
            remote_server_addr=self.endpoint,
            timeout=180,
            init_args_for_pool_manager={"init_args_for_pool_manager": {"retries": False}},
        )
        self.session = webdriver.Remote(self.endpoint, options=options, client_config=config)
        # Initial WDA compilation has its own larger budget. An unavailable
        # driver must not leave diagnostics and teardown waiting indefinitely.
        config.timeout = 30
        if self.platform == "linux":
            # The pinned KDE driver exposes the legacy timeout endpoint.
            self.session.command_executor.add_command(
                "atspiImplicitWait", "POST", "/session/$sessionId/timeouts/implicit_wait"
            )
            self.session.execute("atspiImplicitWait", {"ms": 100})
        else:
            self.session.implicitly_wait(0)
            self.session.update_settings({"fetchFullText": True})

    def quit(self):
        if self.session is not None or (self.platform == "macos" and (self.external_app or self.dialog_active)):
            failures = []
            if self.platform == "macos" and self.external_app and not self.dialog_active:
                try:
                    pids = self.application_pids()
                    assert len(pids) <= 1, "Normal quit requires one owned main process"
                    if pids:
                        helper = Path(__file__).parents[1] / "native/macos/terminate.swift"
                        subprocess.run(["/usr/bin/swift", str(helper), str(self.app), str(next(iter(pids)))],
                                       check=True, capture_output=True, text=True, timeout=30)
                        wait(lambda: not self.application_pids(), "normal owned native client quit")
                except Exception as error:
                    failures.append(error)
                    self.dialog_active = True  # Still clean all owned children after a failed normal quit.
            if self.platform == "macos" and self.dialog_active:
                # On a failed prompt case, snapshotting/terminating the
                # ambiguous XCTest app can itself hang. Clean up only apps
                # from this case's private copied installation before WDA.
                try:
                    from .processes import stop_process_tree

                    assert self.app.resolve().is_relative_to(self.profile.root.resolve())
                    remaining = stop_process_tree(self.application_pids())
                    wait(lambda: not self.application_pids(), "private prompt/client cleanup")
                    self.dialog_active = False
                    if remaining:
                        failures.append(AssertionError(f"Private prompt/client required forced cleanup: {remaining}"))
                except Exception as error:
                    failures.append(error)
            session, self.session = self.session, None
            try:
                if session is not None:
                    session.quit()
            except Exception as error:
                failures.append(error)
            if failures:
                raise AssertionError("Native client cleanup failed: " + "; ".join(map(str, failures))) from failures[0]

    def application_pids(self):
        if self.platform == "macos":
            helper = Path(__file__).parents[1] / "native/macos/applications.swift"
            result = subprocess.run(
                ["/usr/bin/swift", str(helper), str(self.app)],
                check=True, capture_output=True, text=True, timeout=30,
            )
            return set(json.loads(result.stdout))
        result = set()
        for process in Path("/proc").iterdir():
            if not process.name.isdigit():
                continue
            try:
                if (process / "exe").resolve(strict=True) == self.app.resolve():
                    result.add(int(process.name))
            except (FileNotFoundError, PermissionError, ProcessLookupError):
                continue
        return result

    def client_pid(self):
        if self.platform == "linux":
            return self.session.execute_script("linux: applicationPid", {})
        pids = self.application_pids()
        assert len(pids) == 1, f"Expected one process for {self.app}: {pids}"
        return next(iter(pids))

    def activate(self):
        if self.platform == "macos":
            if self.observed_process is None:
                self.session.execute_script("macos: activateApp", {"path": str(self.app)})
            self.activate_owned_client()
            # Bringing a process forward does not guarantee a key window in
            # XCTest. Select its actual title bar, like a user switching clients.
            rectangle = self.window_rect()
            x, y = rectangle["x"] + rectangle["width"] / 2, rectangle["y"] + 12
            if self.observed_process is None:
                self.session.execute_script("macos: click", {"x": x, "y": y})
            else:
                actions = self._actions()
                actions.pointer_action.move_to_location(round(x), round(y)).click()
                actions.perform()
            self.observations.append({"action": "focus_native_window", "rect": rectangle})
            self.activate_owned_client(inspect=True)
        else:
            self.session.execute_script("linux: activateWindow", {})

    def activate_owned_client(self, inspect=False):
        helper = Path(__file__).parents[1] / "native/macos/activate.swift"
        pid = self.client_pid()
        if self.observed_process is not None:
            assert pid == self.observed_process, "The explicitly observed native client changed"
        result = subprocess.run(["/usr/bin/swift", str(helper), str(self.app), str(pid),
                                 *(["--inspect"] if inspect else [])],
                                check=True, capture_output=True, text=True, timeout=30)
        observation = json.loads(result.stdout)
        self.observations.append({"action": "observe_owned_client" if inspect else "activate_owned_client",
                                  **observation})
        assert observation["foreground"] == pid and observation["active"], observation

    def input_source(self, name):
        self.activate()
        if self.platform == "macos":
            identifiers = {"French": "com.apple.keylayout.French", "German": "com.apple.keylayout.German",
                           "Japanese": "com.apple.inputmethod.Kotoeri.RomajiTyping.Japanese"}
            helper = Path(__file__).parents[1] / "native/macos/input-source.swift"
            before = self.native_input_command(["/usr/bin/swift", str(helper), identifiers[name], "--inspect"])
            self.input_sources.append(json.loads(before.stdout))
            result = self.native_input_command(["/usr/bin/swift", str(helper), identifiers[name]])
            observation = json.loads(result.stdout)
            self.native_input_method = observation["selected"] if name == "Japanese" else None
            self.read_layout_keys()
        elif name == "Japanese":
            before = self.ibus_engine()
            self.input_sources.append({"kind": "ibus", "before": before})
            self.ibus_engine("mozc-jp")
            selected = self.ibus_engine()
            assert selected == "mozc-jp", "The real Japanese input engine was not selected"
            observation = {"before": before, "selected": selected, "kind": "ibus"}
        else:
            request = {"layout": {"French": "fr", "German": "de"}[name]}
            self.input_sources.append(self.session.execute_script("linux: keyboardLayout", {
                **request, "inspect": True,
            }))
            observation = self.session.execute_script("linux: keyboardLayout", request)
        self.observations.append({"action": "input_source", **observation})

    def restore_input_source(self):
        while self.input_sources:
            previous = self.input_sources[-1]
            if self.platform == "macos":
                helper = Path(__file__).parents[1] / "native/macos/input-source.swift"
                command = ["/usr/bin/swift", str(helper), previous["before"]]
                command.extend(reversed(previous["restore_disable"]))
                result = self.native_input_command(command)
                observation = json.loads(result.stdout)
                selected = observation["selected"]
                self.native_input_method = selected if selected.startswith("com.apple.inputmethod.") else None
            elif previous.get("kind") == "ibus":
                self.ibus_engine(previous["before"])
                observation = {"selected": self.ibus_engine(), "kind": "ibus"}
            else:
                observation = self.session.execute_script("linux: keyboardLayout", {"layout": previous["before"]})
            assert observation["selected"] == previous["before"], observation
            self.observations.append({"action": "restore_input_source", **observation})
            self.input_sources.pop()
            if self.platform == "macos":
                self.read_layout_keys()

    def read_layout_keys(self):
        helper = Path(__file__).parents[1] / "native/macos/layout-keys.swift"
        result = self.native_input_command(["/usr/bin/swift", str(helper)])
        self.layout_keys = json.loads(result.stdout)
        self.observations.append({"action": "native_layout_keys", "keys": self.layout_keys})

    def native_input_command(self, command):
        result = subprocess.run(command, capture_output=True, text=True, timeout=30)
        with (self.profile.repo / "native-input-source.log").open("a") as log:
            log.write(json.dumps({"command": command, "returncode": result.returncode,
                                  "stdout": result.stdout, "stderr": result.stderr}) + "\n")
        try:
            result.check_returncode()
        except subprocess.CalledProcessError as error:
            error.add_note("Native input-source output:\n" + result.stdout + result.stderr)
            raise
        return result

    def ibus_engine(self, selected=None):
        assert self.profile.env.get("IBUS_ADDRESS") == "unix:path=" + str(self.profile.root / "ibus.sock"), \
            "Japanese input requires its owned IBus fixture"
        helper = Path(__file__).parents[1] / "fixtures/ibus_engine.py"
        result = subprocess.run(["/usr/bin/python3", str(helper), *([selected] if selected else [])],
                                env=self.profile.env, check=True, capture_output=True,
                                text=True, timeout=5)
        return json.loads(result.stdout)["selected"]

    def higher_display_scale(self, surface=("Terminal", "terminal", 0)):
        from .scaling import DisplayScale

        assert self.display_scale_state is None or self.display_scale_state.original is None
        self.display_scale_state = DisplayScale(self, surface)
        self.display_scale_state.start()

    def restore_display_scale(self):
        if self.display_scale_state is not None:
            self.display_scale_state.restore()

    def dialog(self, title):
        if self.platform == "macos":
            # OpenSSH's native prompt is another process from the same
            # bundle/path. XCTest snapshots hang on that ambiguous identity.
            # Observe its owned window pixels without asking XCTest to resolve
            # the ambiguous helper. The main PID is rebound after it exits.
            self.dialog_active = True
            window = self.dialog_window()
            self.dialog_pid = window["pid"]
            from .dialogs import screen_target

            def current_region():
                current = self.dialog_window()
                assert current["pid"] == self.dialog_pid, "The prompt changed while observing its title"
                return current["rect"]

            screen_target(self, title, region=current_region)
            pids = self.application_pids()
            assert len(pids) == 2, f"Expected main client and its private native prompt: {pids}"
            self.dialog_primary = pids - {self.dialog_pid}
            assert len(self.dialog_primary) == 1
            self.observations.append({"action": "native_dialog", "title": title,
                                      "pids": sorted(pids)})
        else:
            self.expect(title)

    def dialog_window(self):
        helper = Path(__file__).parents[1] / "native/macos/dialog-window.swift"

        def visible():
            result = subprocess.run(["/usr/bin/swift", str(helper), str(self.app)],
                                    check=True, capture_output=True, text=True, timeout=30)
            windows = json.loads(result.stdout)
            assert len(windows) <= 1, f"Ambiguous owned native prompt windows: {windows}"
            return windows[0] if windows else False

        window = wait(visible, "OS-owned native prompt window")
        self.observations.append({"action": "dialog_window", **window})
        return window

    def dialog_button(self, title):
        if self.platform == "macos":
            assert self.dialog_active, "No native dialog is being observed"
            window = self.dialog_window()
            assert window["pid"] == self.dialog_pid, "The prompt changed before its action"
            # NSAlert's ordinary Cancel/default keyboard equivalents avoid
            # snapshotting a helper with the same bundle identity as its parent.
            keys = {"Cancel": "ESCAPE", "Continue": "ENTER", "Trust Host": "ENTER"}
            assert title in keys, f"No native dialog keyboard equivalent for {title!r}"
            self.key(keys[title])
            self.observations.append({"action": "native_dialog_key", "title": title,
                                      "key": keys[title], "pid": self.dialog_pid})
            wait(lambda: self.dialog_pid not in self.application_pids(), "acted-on native prompt exits")
        else:
            self.click(title, "button")

    def finish_dialogs(self):
        if self.platform == "macos":
            wait(lambda: self.application_pids() == self.dialog_primary,
                 "authentication completes with the original client")
            # XCTest retains the helper in its process-wide application cache,
            # even after setProcessID changes the object's reported PID. Start
            # a fresh automation runner, preserving the actual main process.
            assert self.external_app, "Native prompt cases must own app lifetime outside XCTest"
            self.activate_owned_client()
            previous_port = self.wda_port
            previous, self.session = self.session, None
            previous.quit()  # skipAppKill avoids touching the stale XCTest app.
            assert self.application_pids() == self.dialog_primary, "Automation disconnected the client"
            with socket.socket() as available:
                available.bind(("127.0.0.1", 0))
                self.wda_port = available.getsockname()[1]
            # The pinned driver's changed systemPort starts a fresh WDA/XCTest
            # process. An app-less session cannot relaunch the product.
            self.launch(attach_existing=True)
            assert self.application_pids() == self.dialog_primary, "Automation changed the client"
            self.attach_process(next(iter(self.dialog_primary)))
            self.observations.append({"action": "reconnect_native_automation",
                                      "before_port": previous_port, "port": self.wda_port,
                                      "pids": sorted(self.dialog_primary)})
            self.dialog_active = False

    def attach_process(self, pid):
        assert self.app.resolve().is_relative_to(self.profile.root.resolve()), \
            "Explicit process ownership requires the case's private installation"
        assert pid in self.application_pids(), "Native attachment requires an observed owned process"
        self.session.execute_script("macos: launchApp", {
            "path": str(self.app), "environment": {"webdriver:attachProcess": pid},
        })
        assert pid in self.application_pids(), "Automation changed the observed native process"
        self.observed_process = pid
        # XCTest can later clear its PID and skip termination on session quit.
        # Own normal shutdown through NSRunningApplication for this private app.
        self.external_app = True
        self.observations.append({"action": "attach_process", "pid": pid})

    def select_word(self, text):
        from .dialogs import screen_target

        rectangle = screen_target(self, text, prefix="selection", bounds=True, terminal=True)
        # Select the printed glyphs with the terminal's ordinary drag gesture.
        # Inset the endpoints so neither lands in a neighbouring cell.
        inset = min(2, rectangle["width"] / (len(text) * 2))
        start = rectangle["x"] + inset
        end = rectangle["x"] + rectangle["width"] - inset
        y = rectangle["y"] + rectangle["height"] / 2
        if self.platform == "macos":
            self.session.execute_script("macos: clickAndDragAndHold", {
                "startX": start, "startY": y, "endX": end, "endY": y,
                "duration": 0.5, "holdDuration": 0.2,
            })
        else:
            actions = ActionBuilder(self.session)
            actions.pointer_action.move_to_location(round(start), round(y))
            actions.pointer_action.pointer_down().pause(0.2)
            actions.pointer_action.move_to_location(round(end), round(y)).pointer_up()
            actions.perform()

    def copy(self):
        self.chord("cmd+c" if self.platform == "macos" else "ctrl+shift+c")

    def choose_directory(self, path):
        if self.platform == "macos":
            self.element("Open Workspace", "button")
            self.chord("cmd+shift+g")

            def go_to_folder_sheet():
                root = ElementTree.fromstring(self.tree())
                return next((sheet for sheet in root.iter("XCUIElementTypeSheet")
                             if sheet.get("identifier") == "GoToWindow"), None)

            def suggested_directory_selected():
                sheet = go_to_folder_sheet()
                if sheet is None:
                    return False
                directory = unicodedata.normalize("NFC", path.name)
                return any(
                    cell.get("selected") == "true"
                    and any(unicodedata.normalize("NFC", label.get("value", "")) == directory
                            for label in cell.iter("XCUIElementTypeStaticText"))
                    for cell in sheet.iter("XCUIElementTypeCell")
                )

            wait(lambda: go_to_folder_sheet() is not None, "native Go to Folder field")
            self.clipboard(str(path))
            self.chord("cmd+a")
            self.chord("cmd+v")
            wait(suggested_directory_selected, "selected native folder completion")
            self.key("ENTER")
            wait(lambda: go_to_folder_sheet() is None, "native Go to Folder sheet to close")
            self.click("Open Workspace", "button")
        else:
            self.chord("ctrl+l")
            self.type(str(path))
            self.key("ENTER")

    def _actions(self, desktop=False):
        if (self.platform == "macos" and (desktop or self.observed_process is not None)
                and not self.dialog_active):
            self.activate_owned_client(inspect=True)
        if self.platform == "macos" and (desktop or self.dialog_active or self.observed_process is not None):
            return ActionBuilder(self.session, mouse=PointerInput("mouse", "desktop-pointer"),
                                 keyboard=KeyInput("desktop-keyboard"))
        return ActionBuilder(self.session)

    def follow_restart(self, previous):
        def restarted():
            current = self.application_pids()
            return current if len(current) == 1 and not current & previous else False

        current = wait(restarted, "updater automatically relaunches the private application", timeout=90)
        if self.platform == "macos":
            # Construct a fresh XCTest application object after the updater's
            # relaunch. Its old object still tracks the terminated PID.
            # Observe the replacement first, so launchApp cannot conceal a
            # missing automatic relaunch by creating one for the product.
            self.session.execute_script("macos: launchApp", {"path": str(self.app)})
        assert self.application_pids() == current, "Automation changed the restarted process"
        self.observations.append({"action": "follow_restart", "previous": sorted(previous),
                                  "current": sorted(current)})

    def tree(self):
        return self.session.page_source

    def texts(self):
        root = ElementTree.fromstring(self.tree())
        values = []
        touchbar = {node for bar in root.iter("XCUIElementTypeTouchBar") for node in bar.iter()}
        for node in root.iter():
            if node in touchbar:
                continue
            attributes = node.attrib
            if self.platform == "macos":
                if (
                    float(attributes.get("width", 0)) <= 0
                    or float(attributes.get("height", 0)) <= 0
                ):
                    continue
            elif "showing" not in attributes.get("states", "").lower():
                continue
            values.append(
                " ".join(
                    attributes.get(key, "")
                    for key in ("name", "label", "title", "value", "description", "text")
                )
            )
        if self.platform == "linux":
            # KDE's XML snapshot omits the AT-SPI Text interface contents.
            # Read visible text widgets through the driver's standard API.
            for element in self.session.find_elements(By.XPATH, "//text | //entry"):
                if element.is_displayed():
                    values.append(self._live_text(element))
        else:
            # XCTest's ordinary snapshots truncate text values at 512
            # characters. Mac2's full-text setting applies to element.text.
            for element in self.session.find_elements(By.CLASS_NAME, "XCUIElementTypeTextView"):
                if element.is_displayed():
                    values.append(self._live_text(element))
            if any(len(node.attrib.get("value", "")) >= 512
                   for node in root.iter("XCUIElementTypeStaticText")):
                # Apple's XPath rejects some long text predicates. Enumerate
                # native text elements rather than embedding their contents.
                for element in self.session.find_elements(By.CLASS_NAME, "XCUIElementTypeStaticText"):
                    if element.is_displayed() and len(element.get_attribute("value") or "") >= 512:
                        values.append(self._live_text(element))
        return "\n".join(values)

    def _live_text(self, element):
        try:
            return element.text
        except InvalidElementStateException as error:
            message = error.msg or ""
            if (self.platform == "macos" and "Cannot extract the full text of " in message
                    and "Code=10008" in message
                    and "No matches found for Descendants matching type " in message):
                # Mac2 misclassifies a replaced text element. Let the existing
                # bounded observer locate its replacement; never replay input.
                raise StaleElementReferenceException(message) from error
            raise

    def has(self, text):
        return text in self.texts()

    def has_exact_label(self, text):
        root = ElementTree.fromstring(self.tree())
        if self.platform == "macos":
            return any(
                text in (node.get("name"), node.get("label"), node.get("value"))
                and float(node.get("width", 0)) > 0 and float(node.get("height", 0)) > 0
                for node in root.iter("XCUIElementTypeStaticText")
            )
        return any(
            text in (node.get("name"), node.get("text"), node.get("value"))
            and "showing" in node.get("states", "").lower()
            for node in root.iter("label")
        )

    def expect(self, text, timeout=20):
        return wait(lambda: self.has(text), repr(text), timeout=timeout)

    def expect_single_sheet(self, heading):
        modal = "XCUIElementTypeSheet" if self.platform == "macos" else "dialog"

        def visible_count():
            root = ElementTree.fromstring(self.tree())
            elements = {node for sheet in root.iter(modal) for node in sheet.iter()
                        if heading in (node.get(key) for key in ("name", "label", "value", "title"))}
            assert len(elements) <= 1, f"Duplicate sheet heading {heading!r}"
            return len(elements) == 1

        wait(visible_count, f"one native sheet with heading {heading!r}")

    def element(self, name, role=None, index=None):
        if role == "machine":
            return self._machine_element(name)
        roles = {
            "button": ("XCUIElementTypeButton", "*[self::push_button or self::button]"),
            "field": ("XCUIElementTypeTextField", "*[self::text or self::entry]"),
            "terminal": ("XCUIElementTypeTextView", "text"),
            "menu": ("XCUIElementTypeMenuItem", "menu_item"),
            "row": ("XCUIElementTypeCell", "*[self::list_item or self::tree_item or self::table_cell]"),
            "disclosure": (
                "XCUIElementTypeDisclosureTriangle",
                "*[self::toggle_button or self::push_button or self::button]",
            ),
            "toggle": (
                "*[self::XCUIElementTypeSwitch or self::XCUIElementTypeCheckBox]",
                "*[self::check_box or self::toggle_button]",
            ),
        }
        tag = roles[role][self.platform == "linux"] if role else "*"
        value = literal(name)
        screen = "[not(ancestor::XCUIElementTypeTouchBar)]" if self.platform == "macos" else ""
        query = f"//{tag}{screen}[@name={value} or @label={value} or @title={value} or @identifier={value} or @accessibility-id={value} or @description={value} or @placeholderValue={value}]"
        sheet = "XCUIElementTypeSheet" if self.platform == "macos" else "dialog"
        # The active upgrade sheet nests inside the Settings sheet, which can
        # still report its own Cancel button as displayed in the native tree.
        modal = f"//{sheet}[not(descendant::{sheet})]"

        def find():
            matches = [e for e in self.session.find_elements(By.XPATH, modal + query) if e.is_displayed()]
            if not matches:
                matches = [e for e in self.session.find_elements(By.XPATH, query) if e.is_displayed()]
            if not matches:
                # SwiftUI exposes adjacent text through value and its control
                # through label. Prefer the semantic control; value is only a
                # fallback for targets such as standalone text and menu items.
                matches = [
                    e
                    for e in self.session.find_elements(By.XPATH, f"//{tag}{screen}[@value={value}]")
                    if e.is_displayed()
                ]
            if self.platform == "macos" and role == "menu" and len(matches) > 1:
                # Closed menu-bar actions are still in XCTest's tree with
                # zero-area bounds. Choose only the real open menu.
                bounds = [(element, element.rect) for element in matches]
                matches = [element for element, rect in bounds if rect["width"] > 0 and rect["height"] > 0]
            if index is not None:
                return matches[index] if len(matches) > index else None
            if len(matches) > 1:
                raise AssertionError(f"Ambiguous accessible target: {name!r}, role={role}")
            return matches[0] if matches else None

        return wait(find, f"accessible target {name!r} ({role})")

    def _machine_element(self, name):
        def find():
            matches = self.machine_elements(name)
            assert len(matches) <= 1, f"Ambiguous machine row {name!r}"
            return matches[0] if matches else None

        return wait(find, f"machine row {name!r}")

    def machine_elements(self, name):
        # SwiftUI combines the machine name with transient connection status.
        # Scope to the machine list's direct scroll children; outline rows can
        # legitimately have the same name as the machine or a local workspace.
        if self.platform == "linux":
            query = f"//*[@accessibility-id={literal('machine.' + name)}]"
        else:
            root = ElementTree.fromstring(self.tree())
            targets = []
            for scroll in root.iter("XCUIElementTypeScrollView"):
                for child in scroll:
                    for key in ("value", "label"):
                        value = child.get(key, "")
                        if value == name or value.startswith(name + ", "):
                            targets.append(
                                f"//XCUIElementTypeScrollView/{child.tag}[@{key}={literal(value)}]"
                            )
                            break
            if not targets:
                return []
            query = " | ".join(targets)
        return [e for e in self.session.find_elements(By.XPATH, query) if e.is_displayed()]

    def allow_runner_network_consent(self):
        """Handle the hosted runner's real OS consent before a remote UI case."""
        assert self.platform == "macos"
        system_app = "com.apple.UserNotificationCenter"
        if self.session.execute_script("macos: queryAppState", {"bundleId": system_app}) <= 1:
            return
        self.session.execute_script("macos: activateApp", {"bundleId": system_app})
        try:
            source = self.tree()
            if "hosted-compute-" in source and "local networks" in source:
                (self.profile.root / "runner-network-consent.xml").write_text(source)
                self.session.save_screenshot(str(self.profile.root / "runner-network-consent.png"))
                self.click("Allow", "button")
                self.observations.append({"action": "allow_runner_local_network_consent"})
        finally:
            self.session.execute_script("macos: activateApp", {"path": str(self.app)})

    def click(self, name, role=None, index=None):
        def enabled():
            target = self.element(name, role, index=index)
            return target if target.is_enabled() else False

        # A terminal is a focus surface, not a command control. XCTest reports
        # some native text areas as disabled even though physical input works.
        element = (wait(enabled, f"enabled click target {name!r}") if role in {"menu", "button"}
                   else self.element(name, role, index=index))
        if self.platform == "macos" and role == "disclosure":
            # SwiftUI's disclosure label ignores the default center click.
            # Target its chevron within the observed accessible control bounds.
            bounds = element.rect
            self.session.execute_script("macos: click", {
                "x": bounds["x"] + bounds["width"] * .4,
                "y": bounds["y"] + bounds["height"] * .5,
            })
            wait(lambda: element.get_attribute("value") == "1", f"expanded {name}")
        else:
            self._click_element(element, desktop=self.platform == "macos" and role == "menu")

    def _click_element(self, element, button=0, desktop=False):
        if self.platform == "macos" and button == 0 and self.observed_process is None and not desktop:
            element.click()
            return
        actions = self._actions(desktop=desktop)
        if self.platform == "linux" or self.observed_process is not None or desktop:
            # Resolve accessible bounds for physical desktop input. This also
            # avoids KDE's top-left interpretation of relative element origins.
            bounds = element.rect
            actions.pointer_action.move_to_location(
                round(bounds["x"] + bounds["width"] / 2),
                round(bounds["y"] + bounds["height"] / 2),
            )
        else:
            actions.pointer_action.move_to(element)
        actions.pointer_action.click(button=button)
        actions.perform()

    def fill(self, name, text):
        self._reveal_field(name)
        field = self.element(name, "field")
        self._click_element(field)
        self.chord("cmd+a" if self.platform == "macos" else "ctrl+a")
        self.key("BACKSPACE")
        if self.platform == "macos" and not text.isascii():
            # Native field editors can reinterpret XCTest's synthesized Unicode
            # key codes through the current layout. Paste exact field contents;
            # raw terminal input and composition retain their physical paths.
            self.clipboard(text)
            self.chord("cmd+v")
        else:
            self.type(text)

    def _reveal_field(self, name):
        """Scroll a real field into its viewport before physical input."""
        root = ElementTree.fromstring(self.tree())
        parents = {child: parent for parent in root.iter() for child in parent}
        tags = {"XCUIElementTypeTextField"} if self.platform == "macos" else {"text", "entry"}
        fields = [node for node in root.iter() if node.tag in tags and name in (
            node.get("name"), node.get("label"), node.get("description"),
            node.get("identifier"), node.get("accessibility-id"), node.get("placeholderValue"))]
        if len(fields) != 1:
            # Ordinary lookup handles not-yet-present and ambiguous controls.
            return
        field = fields[0]
        ancestor = parents.get(field)
        scroll_tag = "XCUIElementTypeScrollView" if self.platform == "macos" else "scroll_pane"
        while ancestor is not None and ancestor.tag != scroll_tag:
            ancestor = parents.get(ancestor)
        if ancestor is None:
            return
        if self.platform == "macos":
            viewport = {key: float(ancestor.get(key, 0)) for key in ("x", "y", "width", "height")}
            target = self.element(name, "field")
        else:
            if "showing" not in ancestor.get("states", "").lower():
                return
            target = self.session.find_element(By.XPATH, f"//*[@path={literal(field.attrib['path'])}]")
            scroller = self.session.find_element(By.XPATH, f"//*[@path={literal(ancestor.attrib['path'])}]")
            viewport = scroller.rect
        assert viewport["width"] > 0 and viewport["height"] > 0, "The field's scroll view is hidden"

        def inside():
            bounds = target.rect
            return (viewport["y"] <= bounds["y"] + bounds["height"] / 2
                    <= viewport["y"] + viewport["height"] and target.is_displayed())

        if inside():
            return
        bounds = target.rect
        center_y = viewport["y"] + viewport["height"] / 2
        distance = bounds["y"] + bounds["height"] / 2 - center_y
        center_x = viewport["x"] + viewport["width"] / 2
        observation = {"action": "reveal_field", "name": name, "viewport": viewport,
                       "before": bounds, "distance": distance}
        self.observations.append(observation)
        if self.platform == "macos":
            self.session.execute_script("macos: scroll", {
                "x": center_x, "y": center_y, "deltaX": 0, "deltaY": -distance,
            })
        else:
            actions = ActionBuilder(self.session)
            actions.wheel_action.scroll(round(center_x), round(center_y), 0, round(distance), 250)
            actions.perform()
        try:
            wait(inside, f"field {name!r} inside its scroll viewport")
        finally:
            try:
                observation["after"] = target.rect
            except Exception as error:  # noqa: BLE001 - preserve the interaction failure
                observation["after_error"] = str(error)

    def field_value(self, name):
        field = self.element(name, "field")
        return field.text if self.platform == "linux" else field.get_attribute("value")

    def type(self, text):
        if not text:
            return
        ordinary = lambda char: char.isascii() or 0xE000 <= ord(char) <= 0xF8FF
        if (self.platform == "macos" and self.native_input_method is None
                and any(not ordinary(char) for char in text)):
            for is_ordinary, characters in groupby(text, key=ordinary):
                chunk = "".join(characters)
                if is_ordinary:
                    self.type(chunk)
                else:
                    self._type_unicode(chunk)
            return
        if self.platform == "macos" and self.native_input_method is not None:
            special = {getattr(Keys, name): name for name in
                       ("ENTER", "ESCAPE", "F6", "TAB", "BACKSPACE", "DELETE", "LEFT", "RIGHT", "UP", "DOWN")}
            request = {"key": special[text]} if text in special else {"text": text}
            helper = Path(__file__).parents[1] / "native/macos/physical-keys.swift"
            result = self.native_input_command([
                "/usr/bin/swift", str(helper), str(self.app), str(self.client_pid()),
                self.native_input_method, json.dumps(request),
            ])
            self.observations.append({"action": "native_physical_input", **json.loads(result.stdout)})
            return
        actions = self._actions()
        if self.platform == "linux":
            # KWin synthesizes the fixture's US physical keymap. GTK's normal
            # Unicode compose sequence commits characters absent from that map.
            for character in text:
                if character.isascii() or 0xE000 <= ord(character) <= 0xF8FF:
                    actions.key_action.send_keys(character)
                else:
                    actions.key_action.key_down(Keys.CONTROL).key_down(Keys.SHIFT)
                    actions.key_action.send_keys("u").key_up(Keys.SHIFT).key_up(Keys.CONTROL)
                    actions.key_action.send_keys(f"{ord(character):x}").send_keys(Keys.ENTER)
        else:
            for character in text:
                physical = self.layout_keys.get(character)
                if physical is None:
                    actions.key_action.send_keys(character)
                    continue
                actions.key_action.key_down(Keys.ALT)
                if physical["shift"]:
                    actions.key_action.key_down(Keys.SHIFT)
                actions.key_action.send_keys(physical["key"])
                if physical["shift"]:
                    actions.key_action.key_up(Keys.SHIFT)
                actions.key_action.key_up(Keys.ALT)
        actions.perform()

    def type_shell_command(self, text):
        # Fixture commands can be long and contain many shifted characters.
        # Post committed text through the native input path on macOS. Cases
        # about physical keys continue to use type, key or chord.
        if self.platform == "macos":
            self._type_unicode(text)
        else:
            self.type(text)

    def _type_unicode(self, text):
        # Avoid XCTest's transient Hex Input source and incomplete surrogate
        # events. Ordinary committed Unicode still crosses the native event
        # boundary; IME composition keeps its separate physical-key path.
        if not self.dialog_active:
            self.activate_owned_client(inspect=True)
        pid = self.dialog_pid if self.dialog_active else self.client_pid()
        helper = Path(__file__).parents[1] / "native/macos/unicode-keys.swift"
        result = self.native_input_command([
            "/usr/bin/swift", str(helper), str(self.app), str(pid), text,
        ])
        self.observations.append({"action": "native_unicode_input", **json.loads(result.stdout)})

    def query_burst(self, name, queries):
        assert queries and all(query.isascii() and query for query in queries)
        self.click(name, "field")
        actions = self._actions()
        modifier = Keys.COMMAND if self.platform == "macos" else Keys.CONTROL
        for query in queries:
            actions.key_action.key_down(modifier).send_keys("a").key_up(modifier)
            actions.key_action.send_keys(query)
        # One physical event stream, with no result waits between replacements.
        actions.perform()
        self.observations.append({"action": "query_burst", "field": name, "queries": list(queries)})

    def open_settings(self):
        if self.platform == "macos":
            self.chord("cmd+,")
        else:
            self.click("settings.open", "button")

    def chord(self, chord):
        parts = chord.split("+")
        modifiers = {
            "cmd": Keys.COMMAND,
            "ctrl": Keys.CONTROL,
            "alt": Keys.ALT,
            "shift": Keys.SHIFT,
        }
        actions = self._actions()
        for modifier in parts[:-1]:
            actions.key_action.key_down(modifiers[modifier])
        actions.key_action.send_keys(parts[-1])
        for modifier in reversed(parts[:-1]):
            actions.key_action.key_up(modifiers[modifier])
        actions.perform()

    def key(self, name):
        self.type(getattr(Keys, name))

    def window_rect(self):
        if self.platform == "linux":
            return self.session.execute_script("linux: windowRect", {})
        windows = self.session.execute_script("macos: listWindows", {})
        main = [window for window in windows if window.get("isMain")]
        if not main and len(windows) == 1:
            main = windows
        assert len(main) == 1, "Expected one main application window"
        return main[0]["rect"]

    def resize(self, width, height):
        if self.platform == "macos":
            current = self.window_rect()
            # CI can restore an 1100-wide window on a 1024-wide display,
            # leaving both resize edges off screen. Bring the left edge into
            # view and leave enough room for an expanding drag.
            origin = 20 + (self.display_origin["x"] if self.display_origin is not None else 0)
            start_left = max(origin, width - current["width"] + origin)
            self._move_window(start_left, current["y"])
            current = self.window_rect()
            if abs(current["width"] - width) > 2:
                # Retina's resize hit area is narrower in logical points.
                # Start just inside its edge; the ordinary scale-1 edge remains
                # the separately proven physical gesture.
                edge = 1 if self.display_origin is not None else -2
                start_x, start_y = current["x"] + edge, current["y"] + current["height"] / 2
                self._window_drag(start_x, start_y,
                                  start_x + current["width"] - width, start_y)
                wait(lambda: abs(self.window_rect()["width"] - width) <= 2,
                     f"native window width {width}")
            self._move_window(origin, current["y"])
            current = self.window_rect()
            if abs(current["height"] - height) > 2:
                start_x, start_y = current["x"] + width / 2, current["y"] + current["height"] - 2
                self._window_drag(start_x, start_y, start_x, current["y"] + height - 2)
        else:
            self.session.execute_script("linux: resizeWindow", {
                "x": 20, "y": 40, "width": width, "height": height,
            })
        wait(lambda: all(abs(self.window_rect()[key] - value) <= 2
                         for key, value in (("width", width), ("height", height))),
             f"native window size {width} by {height}")

    def _window_drag(self, start_x, start_y, end_x, end_y):
        observation = {"action": "window_drag", "before": self.window_rect(),
                       "start": [start_x, start_y], "end": [end_x, end_y]}
        self.observations.append(observation)
        if self.display_origin is not None or self.observed_process is not None:
            # The native coordinate gesture anchors to the AUT screen and
            # stops moving on a secondary Retina output. Viewport event paths
            # preserve global logical coordinates. No pause items: the pinned
            # upstream pointer compiler has a broken pause look-back loop.
            actions = ActionBuilder(self.session, mouse=PointerInput("mouse", "desktop-pointer"))
            actions.pointer_action.move_to_location(round(start_x), round(start_y))
            actions.pointer_action.pointer_down()
            actions.pointer_action.move_to_location(round(end_x), round(end_y))
            actions.pointer_action.pointer_up()
            actions.perform()
        else:
            self.session.execute_script("macos: hover", {"x": start_x, "y": start_y})
            self.session.execute_script("macos: clickAndDragAndHold", {
                "startX": start_x, "startY": start_y, "endX": end_x, "endY": end_y,
                "duration": 0.5, "holdDuration": 0.6,
            })
        observation["after"] = self.window_rect()

    def _move_window(self, x, y):
        current = self.window_rect()
        if abs(current["x"] - x) <= 2 and abs(current["y"] - y) <= 2:
            return
        start_x, start_y = current["x"] + current["width"] / 2, current["y"] + 12
        self._window_drag(start_x, start_y, start_x + x - current["x"], start_y + y - current["y"])
        # AppKit can constrain the origin by a few points when fitting an
        # oversized restored window onto the hosted runner's small display.
        wait(lambda: abs(self.window_rect()["x"] - x) <= 4 and abs(self.window_rect()["y"] - y) <= 4,
             f"native window position {x}, {y}")

    def begin_composition(self):
        assert self.composition is None, "Composition is already in progress"
        if self.platform == "macos":
            # The native US/ABC dead key starts AppKit marked text.
            self.chord("alt+e")
        else:
            # GTK's built-in input method keeps these digits as preedit until
            # Enter commits them. This crosses the same toolkit IM boundary.
            self.chord("ctrl+shift+u")
            self.type("e9")
        self.composition = "acute-e"

    def finish_composition(self, commit=True):
        assert self.composition == "acute-e", "No native composition is in progress"
        if commit:
            self.type("e") if self.platform == "macos" else self.key("ENTER")
        else:
            self.key("ESCAPE")
        self.composition = None

    def context(self, name, role=None):
        self._click_element(self.element(name, role), button=2)

    def middle(self, name):
        self._click_element(self.element(name), button=1)

    def drag(self, source, target, target_x=0.5, target_y=0.5, role=None):
        start_element, end_element = self.element(source, role), self.element(target, role)
        if self.platform == "macos":
            # Mac2 4.6.1's W3C pointer-move synthesizer loops indefinitely
            # when the preceding item is a pause. Use its native XCTest
            # gesture, resolving both accessible rectangles before pressing.
            start, end = start_element.rect, end_element.rect
            self.session.execute_script(
                "macos: clickAndDragAndHold",
                {
                    "startX": start["x"] + start["width"] / 2,
                    "startY": start["y"] + start["height"] / 2,
                    "endX": end["x"] + end["width"] * target_x,
                    "endY": end["y"] + end["height"] * target_y,
                    "duration": 0.5,
                    "holdDuration": 0.6,
                },
            )
            return
        actions = ActionBuilder(self.session)
        start, end = start_element.rect, end_element.rect
        actions.pointer_action.move_to_location(
            round(start["x"] + start["width"] / 2),
            round(start["y"] + start["height"] / 2),
        ).pointer_down().pause(0.5).move_to_location(
            round(end["x"] + end["width"] * target_x),
            round(end["y"] + end["height"] * target_y),
        ).pause(0.6).pointer_up()
        actions.perform()

    def clipboard(self, text=None):
        if self.clipboard_owner is not self:
            return self.clipboard_owner.clipboard(text)
        if self.platform == "linux":
            import base64
            from .clipboard import linux_clipboard

            text_mime = "text/plain;charset=utf-8"
            if text is None:
                saved = linux_clipboard("capture", self.profile)
                for mime in (text_mime, "text/plain", "UTF8_STRING"):
                    if mime in saved:
                        return base64.b64decode(saved[mime]).decode("utf-8")
                return ""
            if self.saved_binary_clipboard is None:
                self.saved_binary_clipboard = linux_clipboard("capture", self.profile)
            return linux_clipboard("restore", self.profile, {
                text_mime: base64.b64encode(text.encode("utf-8")).decode("ascii"),
            })
        if text is None:
            return subprocess.run(
                ["/usr/bin/pbpaste"], check=True, capture_output=True, text=True, timeout=10
            ).stdout
        if self.saved_clipboard is None:
            self.saved_clipboard = self._mac_clipboard("capture")
        subprocess.run(["/usr/bin/pbcopy"], input=text, text=True, check=True, timeout=10)

    def restore_clipboard(self):
        if self.saved_binary_clipboard is not None:
            from .clipboard import linux_clipboard

            linux_clipboard("restore", self.profile, self.saved_binary_clipboard)
            self.saved_binary_clipboard = None
        if self.saved_clipboard is not None:
            self._mac_clipboard("restore", self.saved_clipboard)
            self.saved_clipboard = None

    def clipboard_image(self, kind, image, text=None):
        from .clipboard import linux_clipboard, representations

        owner = self.clipboard_owner
        contents = representations(self.platform, kind, image, text)
        if self.platform == "macos":
            if owner.saved_clipboard is None:
                owner.saved_clipboard = owner._mac_clipboard("capture")
            owner._mac_clipboard("restore", contents)
        else:
            if owner.saved_binary_clipboard is None:
                owner.saved_binary_clipboard = linux_clipboard("capture", owner.profile)
            linux_clipboard("restore", owner.profile, contents)

    def _mac_clipboard(self, action, data=None):
        helper = Path(__file__).parents[1] / "native/macos/clipboard.swift"
        result = subprocess.run(
            ["/usr/bin/swift", str(helper), action],
            input=json.dumps(data) if data is not None else None,
            text=True,
            capture_output=True,
            check=True,
            timeout=30,
        )
        return json.loads(result.stdout) if action == "capture" else None
