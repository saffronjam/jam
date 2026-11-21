"""Native drivers share WebDriver operations, not product decision logic."""

import json
import plistlib
import shlex
import subprocess
import time
from pathlib import Path
from xml.etree import ElementTree

from appium import webdriver
from appium.options.common import AppiumOptions
from appium.webdriver.client_config import AppiumClientConfig
from selenium.common.exceptions import NoSuchElementException, StaleElementReferenceException
from selenium.webdriver.common.actions.action_builder import ActionBuilder
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

    def launch(self, directory=None):
        options = AppiumOptions()
        arguments = ["--directory", str(directory or self.profile.repo)]
        if self.platform == "macos":
            with (self.app / "Contents/Info.plist").open("rb") as info:
                bundle_id = plistlib.load(info)["CFBundleIdentifier"]
            caps = {
                "platformName": "mac",
                "appium:automationName": "mac2",
                "appium:appPath": str(self.app),
                "appium:bundleId": bundle_id,
                "appium:arguments": arguments,
                "appium:environment": self.profile.env,
            }
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
        if self.session is not None:
            session, self.session = self.session, None
            session.quit()

    def tree(self):
        return self.session.page_source

    def texts(self):
        root = ElementTree.fromstring(self.tree())
        values = []
        for node in root.iter():
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
                    values.append(element.text)
        else:
            # XCTest's ordinary snapshots truncate text values at 512
            # characters. Mac2's full-text setting applies to element.text.
            query = "//XCUIElementTypeTextView | //XCUIElementTypeStaticText[string-length(@value)>=512]"
            for element in self.session.find_elements(By.XPATH, query):
                if element.is_displayed():
                    values.append(element.text)
        return "\n".join(values)

    def has(self, text):
        return text in self.texts()

    def expect(self, text):
        return wait(lambda: self.has(text), repr(text))

    def element(self, name, role=None, index=None):
        roles = {
            "button": ("XCUIElementTypeButton", "*[self::push_button or self::button]"),
            "field": ("XCUIElementTypeTextField", "*[self::text or self::entry]"),
            "terminal": ("XCUIElementTypeTextView", "text"),
            "menu": ("XCUIElementTypeMenuItem", "menu_item"),
            "toggle": (
                "*[self::XCUIElementTypeSwitch or self::XCUIElementTypeCheckBox]",
                "*[self::check_box or self::toggle_button]",
            ),
        }
        tag = roles[role][self.platform == "linux"] if role else "*"
        value = literal(name)
        query = f"//{tag}[@name={value} or @label={value} or @title={value} or @identifier={value} or @accessibility-id={value} or @description={value} or @placeholderValue={value}]"

        def find():
            matches = [e for e in self.session.find_elements(By.XPATH, query) if e.is_displayed()]
            if not matches:
                # SwiftUI exposes adjacent text through value and its control
                # through label. Prefer the semantic control; value is only a
                # fallback for targets such as standalone text and menu items.
                matches = [
                    e
                    for e in self.session.find_elements(By.XPATH, f"//{tag}[@value={value}]")
                    if e.is_displayed()
                ]
            if index is not None:
                return matches[index] if len(matches) > index else None
            if len(matches) > 1:
                raise AssertionError(f"Ambiguous accessible target: {name!r}, role={role}")
            return matches[0] if matches else None

        return wait(find, f"accessible target {name!r} ({role})")

    def click(self, name, role=None, index=None):
        self._click_element(self.element(name, role, index=index))

    def _click_element(self, element, button=0):
        if self.platform == "macos" and button == 0:
            element.click()
            return
        actions = ActionBuilder(self.session)
        if self.platform == "linux":
            # The pinned KDE driver treats element origins as the top-left,
            # whereas W3C actions define their origin as the element's center.
            # Resolve accessible bounds explicitly, then inject real input.
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
        field = self.element(name, "field")
        self._click_element(field)
        self.chord("cmd+a" if self.platform == "macos" else "ctrl+a")
        self.key("BACKSPACE")
        self.type(text)

    def field_value(self, name):
        field = self.element(name, "field")
        return field.text if self.platform == "linux" else field.get_attribute("value")

    def type(self, text):
        if not text:
            return
        actions = ActionBuilder(self.session)
        actions.key_action.send_keys(text)
        actions.perform()

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
        actions = ActionBuilder(self.session)
        for modifier in parts[:-1]:
            actions.key_action.key_down(modifiers[modifier])
        actions.key_action.send_keys(parts[-1])
        for modifier in reversed(parts[:-1]):
            actions.key_action.key_up(modifiers[modifier])
        actions.perform()

    def key(self, name):
        self.type(getattr(Keys, name))

    def context(self, name):
        self._click_element(self.element(name), button=2)

    def middle(self, name):
        self._click_element(self.element(name), button=1)

    def drag(self, source, target, target_x=0.5, target_y=0.5):
        start_element, end_element = self.element(source), self.element(target)
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
        if text is None:
            if self.platform == "macos":
                return subprocess.run(
                    ["/usr/bin/pbpaste"], check=True, capture_output=True, text=True, timeout=10
                ).stdout
            return self.session.get_clipboard_text()
        if self.saved_clipboard is None:
            if self.platform == "macos":
                self.saved_clipboard = self._mac_clipboard("capture")
            else:
                self.saved_clipboard = self.clipboard()
        if self.platform == "macos":
            subprocess.run(["/usr/bin/pbcopy"], input=text, text=True, check=True, timeout=10)
            return
        self.session.set_clipboard_text(text)

    def restore_clipboard(self):
        if self.saved_clipboard is not None:
            if self.platform == "macos":
                self._mac_clipboard("restore", self.saved_clipboard)
            else:
                self.clipboard(self.saved_clipboard)
            self.saved_clipboard = None

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
