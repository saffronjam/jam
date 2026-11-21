"""Native drivers share WebDriver operations, not product decision logic."""

import shlex
import subprocess
import time
from xml.etree import ElementTree

from appium import webdriver
from appium.options.common import AppiumOptions
from appium.webdriver.client_config import AppiumClientConfig
from selenium.common.exceptions import NoSuchElementException, StaleElementReferenceException
from selenium.webdriver.common.action_chains import ActionChains
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
            caps = {
                "platformName": "mac",
                "appium:automationName": "mac2",
                "appium:appPath": str(self.app),
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
        config = AppiumClientConfig(remote_server_addr=self.endpoint, timeout=180)
        self.session = webdriver.Remote(self.endpoint, options=options, client_config=config)
        # Initial WDA compilation has its own larger budget. An unavailable
        # driver must not leave diagnostics and teardown waiting indefinitely.
        config.timeout = 30
        self.session.implicitly_wait(0)

    def quit(self):
        if self.session is not None:
            session, self.session = self.session, None
            session.quit()

    def tree(self):
        return self.session.page_source

    def texts(self):
        root = ElementTree.fromstring(self.tree())
        return "\n".join(
            " ".join(
                node.attrib.get(k, "") for k in ("name", "label", "value", "description", "text")
            )
            for node in root.iter()
        )

    def has(self, text):
        return text in self.texts()

    def expect(self, text):
        return wait(lambda: self.has(text), repr(text))

    def element(self, name, role=None, index=None):
        roles = {
            "button": ("XCUIElementTypeButton", "push_button"),
            "field": ("XCUIElementTypeTextField", "text"),
            "terminal": ("XCUIElementTypeTextView", "text"),
            "menu": ("XCUIElementTypeMenuItem", "menu_item"),
            "toggle": ("XCUIElementTypeSwitch", "check_box"),
        }
        tag = roles[role][self.platform == "linux"] if role else "*"
        value = literal(name)
        query = f"//{tag}[@name={value} or @label={value} or @identifier={value} or @description={value} or @placeholderValue={value}]"

        def find():
            matches = [e for e in self.session.find_elements(By.XPATH, query) if e.is_displayed()]
            if index is not None:
                return matches[index] if len(matches) > index else None
            if len(matches) > 1:
                raise AssertionError(f"Ambiguous accessible target: {name!r}, role={role}")
            return matches[0] if matches else None

        return wait(find, f"accessible target {name!r} ({role})")

    def click(self, name, role=None):
        self.element(name, role).click()

    def fill(self, name, text):
        field = self.element(name, "field")
        field.click()
        self.chord("cmd+a" if self.platform == "macos" else "ctrl+a")
        self.type(text)

    def type(self, text):
        ActionChains(self.session).send_keys(text).perform()

    def chord(self, chord):
        parts = chord.split("+")
        modifiers = {
            "cmd": Keys.COMMAND,
            "ctrl": Keys.CONTROL,
            "alt": Keys.ALT,
            "shift": Keys.SHIFT,
        }
        action = ActionChains(self.session)
        for modifier in parts[:-1]:
            action.key_down(modifiers[modifier])
        action.send_keys(parts[-1])
        for modifier in reversed(parts[:-1]):
            action.key_up(modifiers[modifier])
        action.perform()

    def key(self, name):
        self.type(getattr(Keys, name))

    def context(self, name):
        ActionChains(self.session).context_click(self.element(name)).perform()

    def drag(self, source, target, x=0, y=0):
        ActionChains(self.session).move_to_element(self.element(source)).click_and_hold().pause(
            0.5
        ).move_to_element_with_offset(self.element(target), x, y).pause(0.6).release().perform()

    def clipboard(self, text=None):
        if text is None:
            if self.platform == "macos":
                return subprocess.run(
                    ["/usr/bin/pbpaste"], check=True, capture_output=True, text=True, timeout=10
                ).stdout
            return self.session.get_clipboard_text()
        if self.saved_clipboard is None:
            self.saved_clipboard = self.clipboard()
        if self.platform == "macos":
            subprocess.run(["/usr/bin/pbcopy"], input=text, text=True, check=True, timeout=10)
            return
        self.session.set_clipboard_text(text)

    def restore_clipboard(self):
        if self.saved_clipboard is not None:
            self.clipboard(self.saved_clipboard)
            self.saved_clipboard = None
