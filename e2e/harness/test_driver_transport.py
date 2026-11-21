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


def test_disconnected_source_request_is_not_retried(tmp_path):
    requests = []

    class Handler(BaseHTTPRequestHandler):
        def log_message(self, *_):
            pass

        def do_POST(self):
            self.rfile.read(int(self.headers.get("Content-Length", 0)))
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
            SimpleNamespace(repo=tmp_path, env={}),
        )
        try:
            ui.launch()
            with pytest.raises(ProtocolError):
                ui.tree()
            assert requests == ["/session/native-session/source"]
        finally:
            ui.quit()
            server.shutdown()
            thread.join(timeout=2)
