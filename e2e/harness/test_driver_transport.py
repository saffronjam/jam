"""A disconnected native driver must fail once instead of replaying HTTP commands."""

import json
import plistlib
import socket
import threading
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from types import SimpleNamespace

import pytest
from urllib3.exceptions import ProtocolError

from jam_e2e.driver import Desktop


@pytest.mark.parametrize("attach_existing", [False, True], ids=["normal", "automation-only"])
def test_disconnected_source_request_is_not_retried(tmp_path, attach_existing):
    requests = []
    launches = []

    class Handler(BaseHTTPRequestHandler):
        def log_message(self, *_):
            pass

        def do_POST(self):
            body = self.rfile.read(int(self.headers.get("Content-Length", 0)))
            if self.path == "/session":
                launches.append(json.loads(body)["capabilities"]["alwaysMatch"])
            self.reply(
                {"sessionId": "native-session", "capabilities": {"platformName": "mac"}}
                if self.path == "/session"
                else None
            )

        def do_DELETE(self):
            self.reply(None)

        def do_GET(self):
            requests.append(self.path)
            self.connection.shutdown(socket.SHUT_RDWR)
            self.connection.close()

        def reply(self, value):
            data = json.dumps({"value": value}).encode()
            self.send_response(200)
            self.send_header("Content-Length", str(len(data)))
            self.end_headers()
            self.wfile.write(data)

    app = tmp_path / "Probe.app"
    (app / "Contents").mkdir(parents=True)
    (app / "Contents/Info.plist").write_bytes(
        plistlib.dumps({"CFBundleIdentifier": "fixture.native-probe"})
    )
    with ThreadingHTTPServer(("127.0.0.1", 0), Handler) as server:
        thread = threading.Thread(target=server.serve_forever, daemon=True)
        thread.start()
        ui = Desktop(
            "macos",
            f"http://127.0.0.1:{server.server_port}",
            app,
            SimpleNamespace(root=tmp_path, repo=tmp_path, env={}),
        )
        if attach_existing:
            ui.external_app = True
            ui.wda_port = 10101
            # This transport check owns no macOS application on either OS.
            ui.application_pids = lambda: set()
        try:
            ui.launch(attach_existing=attach_existing)
            assert len(launches) == 1
            if attach_existing:
                assert launches[0]["appium:skipAppKill"] is True
                assert launches[0]["appium:systemPort"] == 10101
                assert not {"appium:appPath", "appium:bundleId", "appium:arguments", "appium:environment"} & launches[0].keys()
            else:
                assert launches[0]["appium:appPath"] == str(app)
            with pytest.raises(ProtocolError):
                ui.tree()
            assert requests == ["/session/native-session/source"]
        finally:
            ui.quit()
            server.shutdown()
            thread.join(timeout=2)
