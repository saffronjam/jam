import json
import os
import subprocess
import uuid
from pathlib import Path


class Profile:
    def __init__(self, root: Path, cli: Path, daemon: Path):
        self.root, self.cli, self.daemon = root, cli, daemon
        self.home = root / "home"
        self.config = root / "config" / "jam" / "config.toml"
        self.state = root / "state"
        self.repo = self.home / "Project ü with spaces"
        self.socket = root / "run" / "jam.sock"
        for directory in (self.home, self.config.parent, self.state, self.socket.parent, self.repo):
            directory.mkdir(parents=True, exist_ok=True, mode=0o700)
        self.env = dict(
            os.environ,
            HOME=str(self.home),
            XDG_CONFIG_HOME=str(root / "config"),
            XDG_STATE_HOME=str(root / "xdg-state"),
            XDG_DATA_HOME=str(root / "data"),
            XDG_RUNTIME_DIR=str(self.socket.parent),
            JAM_STATE_DIR=str(self.state),
            JAM_SOCKET=str(self.socket),
            JAM_DAEMON=str(daemon),
            JAM_DEFAULTS_SUITE="dev.saffronjam.e2e." + uuid.uuid4().hex,
            GIT_CONFIG_GLOBAL=os.devnull,
            GIT_CONFIG_NOSYSTEM="1",
            GIT_AUTHOR_NAME="E2E",
            GIT_AUTHOR_EMAIL="e2e@example.invalid",
            GIT_COMMITTER_NAME="E2E",
            GIT_COMMITTER_EMAIL="e2e@example.invalid",
            SHELL="/bin/bash",
            JAM_E2E_CLI=str(cli),
        )
        # Never inherit agent credentials into a fixture shell.
        for key in list(self.env):
            if any(token in key for token in ("TOKEN", "API_KEY", "SECRET", "AUTHORIZATION")):
                self.env.pop(key)
        self.env.pop("SSH_AUTH_SOCK", None)
        for key in (
            "JAM_AGENT_ID",
            "JAM_AGENT_RUN_ID",
            "JAM_PANE_ID",
            "JAM_WORKSPACE_ID",
            "JAM_REMOTE",
        ):
            self.env.pop(key, None)
        self.env["PATH"] = os.pathsep.join(
            (str(root / "bin"), str(cli.parent), "/usr/bin", "/bin", "/usr/sbin", "/sbin")
        )
        (root / "bin").mkdir()
        self.env.pop("CLAUDE_CONFIG_DIR", None)
        self.env["CODEX_HOME"] = str(self.home / ".codex")
        self.env["CLAUDE_CONFIG_DIR"] = str(self.home / ".claude")
        display = os.environ.get("WAYLAND_DISPLAY")
        runtime = os.environ.get("XDG_RUNTIME_DIR")
        if display and runtime and not os.path.isabs(display):
            # The app keeps private runtime files while reaching the runner's
            # isolated compositor through an absolute Wayland socket path.
            self.env["WAYLAND_DISPLAY"] = str(Path(runtime) / display)
        self.git("init", "-b", "main")
        (self.repo / "tracked.txt").write_text("first\nsecond\n")
        self.git("add", ".")
        self.git("commit", "-m", "Fixture")

    def git(self, *args):
        return subprocess.run(
            ["git", *args],
            cwd=self.repo,
            env=self.env,
            check=True,
            capture_output=True,
            text=True,
            timeout=30,
        ).stdout

    def call(self, *args):
        try:
            result = subprocess.run(
                [str(self.cli), "--json", *args],
                env=self.env,
                capture_output=True,
                text=True,
                timeout=30,
                check=True,
            )
        except subprocess.CalledProcessError as error:
            error.add_note("Public CLI error: " + error.stderr)
            raise
        return json.loads(result.stdout)

    def layout(self):
        return self.call("workspace", "list")["State"]

    def capture(self, pane):
        return self.call("pane", "capture", "--pane", pane)["Capture"]

    def write_config(self, text):
        self.config.write_text(text)

    def stop(self):
        if self.socket.exists():
            self.call("daemon", "stop")
