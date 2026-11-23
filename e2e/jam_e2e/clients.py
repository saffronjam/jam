"""Own independent native client processes sharing one isolated product profile."""

import subprocess

from .driver import Desktop, wait


class Clients:
    def __init__(self, primary):
        self.primary = primary
        self.members = {"primary": primary}
        self.pids = {"primary": primary.client_pid()}

    def open(self, name):
        assert name not in self.members, f"Client already exists: {name}"
        app = self.primary.app
        if self.primary.platform == "macos":
            # Distinct bundle paths let XCTest address independent processes
            # with the same bundle ID. Both still use the exact same profile.
            app = self.primary.profile.root / ("client-" + name) / "Jam.app"
            app.parent.mkdir()
            subprocess.run(["/usr/bin/ditto", str(self.primary.app), str(app)],
                           check=True, capture_output=True, timeout=60)
        peer = Desktop(self.primary.platform, self.primary.endpoint, app, self.primary.profile)
        peer.clipboard_owner = self.primary
        self.members[name] = peer
        if peer.platform == "macos":
            peer.session = self.primary.session
            # XCTest stalled for four minutes without spawning the first
            # same-ID secondary bundle. Use the ordinary OS launcher, then
            # bind XCTest to an independently observed running process.
            command = ["/usr/bin/open", "-n"]
            for key, value in peer.profile.env.items():
                command += ["--env", key + "=" + value]
            command += [str(app), "--args", "--directory", str(peer.profile.repo)]
            subprocess.run(command, check=True, capture_output=True, timeout=20)
            pids = wait(lambda: peer.application_pids(), "secondary native application starts")
            assert len(pids) == 1, "The OS launcher created multiple secondary clients"
            peer.session.execute_script("macos: launchApp", {"path": str(app)})
            assert peer.application_pids() == pids, "XCTest changed the secondary process"
        else:
            peer.launch()
        pid = peer.client_pid()
        assert pid not in self.pids.values(), "Second client reused an existing application process"
        self.pids[name] = pid
        self.primary.observations.append({"action": "open_client", "name": name, "pid": pid,
                                          "primary_pid": self.pids["primary"]})
        return self.select(name)

    def select(self, name):
        ui = self.members[name]
        ui.activate()
        assert ui.client_pid() == self.pids[name], "Native client ownership changed"
        return ui

    def close(self, name):
        assert name != "primary", "Keep the primary client for fixture teardown"
        ui = self.members[name]
        if ui.session is not None:
            if ui.platform == "macos":
                ui.session.execute_script("macos: terminateApp", {"path": str(ui.app)})
                ui.session = None
                wait(lambda: not ui.application_pids(), "owned secondary client terminates")
            else:
                ui.quit()
        del self.members[name]
        self.pids.pop(name, None)
        return self.select("primary")

    def close_all(self):
        failures = []
        for name in list(self.members):
            if name != "primary":
                try:
                    self.close(name)
                except Exception as error:
                    failures.append(f"{name}: {error}")
        assert not failures, "Secondary client cleanup failed: " + "; ".join(failures)
