import re
import shlex
import tomllib
import uuid

from .driver import wait


class Jam:
    def __init__(self, ui, profile):
        self.ui, self.profile = ui, profile

    def command(self, title):
        self.ui.chord("ctrl+shift+p")
        self.ui.fill("Search commands, workspaces, tabs, and panes", title)
        self.ui.key("ENTER")

    def terminal(self):
        self.ui.element("Terminal", "terminal", index=0).click()

    def run(self, command):
        self.terminal()
        # New panes have fresh shell settings. Disable echo in a separate turn
        # so the command under assertion cannot appear merely as typed input.
        self._shell("stty -echo; PS1='$ '; PS2='> '; PROMPT_COMMAND=''")
        self._shell(command)

    def _shell(self, command):
        marker = "JAM_RESULT_" + uuid.uuid4().hex
        # The contiguous result marker does not occur in the typed command.
        self.ui.type(
            command
            + "; printf '\\n%s%s\\n' "
            + shlex.quote(marker[:12])
            + " "
            + shlex.quote(marker[12:])
        )
        self.ui.key("ENTER")
        wait(
            lambda: any(
                marker in self.profile.capture(p["id"]) for p in self.profile.layout()["panes"]
            ),
            "shell command completion",
        )

    def settings(self, category="General"):
        self.command("Settings")
        self.ui.click(category, "button")

    def save(self):
        self.ui.click("Save", "button")

    def config(self):
        return tomllib.loads(self.profile.config.read_text())

    def expect_config(self, key, value):
        def matches():
            if not self.profile.config.exists():
                return False
            current = self.config()
            for part in key.split("."):
                current = current[part]
            return current == value

        wait(matches, f"persisted {key}={value!r}")

    def count(self, kind, expected):
        def matches():
            state = self.profile.layout()
            objects = (
                state[kind]
                if kind != "tabs"
                else [t for w in state["workspaces"] for t in w["tabs"]]
            )
            return len(objects) == expected

        wait(matches, f"{expected} {kind}")

    def pids(self):
        panes = [p for p in self.profile.layout()["panes"] if not p["exited"]]
        result = {}
        for pane in panes:
            marker = "PID_" + uuid.uuid4().hex
            self.profile.call(
                "pane",
                "send",
                "--pane",
                pane["id"],
                "--text",
                f"printf '\\n{marker}:%s\\n' \"$$\"",
                "--enter",
            )
            values = wait(
                lambda marker=marker, pane=pane: re.findall(
                    r"^" + marker + r":(\d+)\s*$", self.profile.capture(pane["id"]), re.MULTILINE
                ),
                "shell PID probe",
            )
            result[pane["id"]] = values[-1]
        return result
