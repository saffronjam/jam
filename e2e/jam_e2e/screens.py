import json
import tomllib
import uuid

from .driver import wait
from .processes import pane_processes


class Jam:
    def __init__(self, ui, profile):
        self.ui, self.profile = ui, profile
        self.clients = None

    def close_clients(self):
        if self.clients is not None:
            self.clients.close_all()

    def command(self, title):
        self.ui.chord("ctrl+shift+p")
        self.ui.fill("Search commands, workspaces, tabs, and panes", title)
        self.ui.key("ENTER")

    def terminal(self):
        self.ui.click("Terminal", "terminal", index=0)

    def open_workspace(self):
        self.ui.chord("cmd+o" if self.ui.platform == "macos" else "ctrl+o")

    def remote_shell(self, command=""):
        from .remote import peer_call

        # Initial SSH inspection, socket discovery and forwarding can outlast
        # an ordinary element lookup on the real TCG guest. Observe startup
        # before sending input; the command and its exact result run once.
        self.ui.expect("Remote project", timeout=90)
        self.ui.click("Remote project", "row")
        wait(lambda: self.ui.element("Terminal", "terminal", index=0).is_enabled(),
             "remote terminal accepts input", timeout=90)
        self.terminal()
        marker = "REMOTE_UI_" + uuid.uuid4().hex
        self.ui.type_shell_command((command + "; " if command else "") + "echo; echo "
                                   + marker[:12] + "'" + marker[12:] + "'")
        self.ui.key("ENTER")

        def executed():
            state = peer_call(self.profile, "workspace", "list")["State"]
            captures = {pane["id"]: peer_call(
                self.profile, "pane", "capture", "--pane", pane["id"],
            )["Capture"] for pane in state["panes"]}
            with (self.profile.root / "fixture-remote-input.log").open("a") as log:
                log.write(json.dumps({"marker": marker, "captures": captures}) + "\n")
            return any(marker in text for text in captures.values())

        # TCG guests can take more than 20s to drain a long sequence of real
        # key events. Observe independently without rebuilding CLI SSH tunnels.
        wait(executed, "command physically entered through the remote UI terminal", timeout=60)

    def run(self, command):
        self.terminal()
        self.run_focused(command)

    def run_focused(self, command):
        # New panes have fresh shell settings. Disable echo in a separate turn
        # so the command under assertion cannot appear merely as typed input.
        self._shell("stty -echo; PS1='$ '; PS2='> '; PROMPT_COMMAND=''")
        self._shell(command)

    def _shell(self, command):
        marker = "JAM_RESULT_" + uuid.uuid4().hex
        # The contiguous result marker does not occur in the typed command.
        self.ui.type_shell_command(
            command
            + "; echo; echo "
            + marker[:12]
            + "'" + marker[12:] + "'"
        )
        self.ui.key("ENTER")
        wait(
            lambda: any(
                marker in self.profile.capture(p["id"]) for p in self.profile.layout()["panes"]
            ),
            "shell command completion",
        )

    def settings(self, category="General"):
        # Settings must remain reachable while testing palette reassignment
        # and unbinding, through each desktop's ordinary settings entry.
        self.ui.open_settings()
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
