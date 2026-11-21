import shlex
import tomllib
import uuid

from .driver import wait
from .processes import pane_processes


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
        return pane_processes(self.profile)
