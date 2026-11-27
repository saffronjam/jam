"""Real local HTTPS dependency services for externally driven application tests."""

import argparse
import hashlib
import json
import ssl
import subprocess
import time
import uuid
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from socketserver import TCPServer
from pathlib import Path
from urllib.parse import urlsplit


class LoopbackHTTPServer(ThreadingHTTPServer):
    def server_bind(self):
        # HTTPServer.server_bind performs reverse DNS even for 127.0.0.1.
        # A real Mac stack sample showed that lookup stalled before readiness.
        TCPServer.server_bind(self)
        self.server_name = "localhost"
        self.server_port = self.server_address[1]


def load_config(path):
    config = json.loads(path.read_text())
    for key in ("certificate", "ca_certificate", "tls_key", "signing_key", "public_key"):
        config[key] = str((path.parent / config[key]).resolve())
    return config


def signed_manifest(directory, config, version, protocol, assets):
    manifest = {
        "schema": 1,
        "version": version,
        "protocol": str(protocol),
        "min_macos": "14.0",
        "assets": [],
    }
    for artifact in assets:
        content = Path(artifact["path"]).read_bytes()
        manifest["assets"].append(
            {
                "name": artifact["name"],
                "platform": artifact["platform"],
                "arch": artifact["arch"],
                "sha256": hashlib.sha256(content).hexdigest(),
                "size": len(content),
            }
        )
    data = (json.dumps(manifest, sort_keys=True, indent=2) + "\n").encode()
    directory.mkdir(parents=True, exist_ok=True)
    path = directory / "manifest.json"
    path.write_bytes(data)
    signature = directory / "signature.bin"
    subprocess.run(
        [
            "openssl",
            "pkeyutl",
            "-sign",
            "-rawin",
            "-inkey",
            config["signing_key"],
            "-in",
            str(path),
            "-out",
            str(signature),
        ],
        check=True,
        capture_output=True,
        timeout=15,
    )
    (directory / "manifest.sig").write_text(signature.read_bytes().hex() + "\n")
    for asset in assets:
        (directory / asset["name"]).write_bytes(Path(asset["path"]).read_bytes())
    return manifest


CLAUDE_ORG = "abababab-cdcd-4ded-8efe-121212121212"


def handler(root, scenario, events):
    spent = events.with_suffix(".claude-reset")
    class Handler(BaseHTTPRequestHandler):
        def log_message(self, *_):
            pass

        def do_GET(self):
            state = json.loads(scenario.read_text())
            path = urlsplit(self.path).path
            # Record only the origin marker reserved for the synthetic remote
            # login. Never retain authorization headers or credential bytes.
            auth = self.headers.get("Authorization", "")
            remote_login = auth.endswith(".fixture-remote")
            separate_usage = state.get("distinct_organization_usage", False)
            with events.open("a") as output:
                output.write(
                    json.dumps(
                        {"path": path, "authenticated": bool(auth),
                         "fixture_host": "remote" if remote_login else "local"}
                    )
                    + "\n"
                )
            if state.get("delay"):
                time.sleep(min(15, state["delay"]))
            if state.get("http_error"):
                self.send_error(int(state["http_error"]))
                return
            if remote_login and state.get("remote_auth_error") and path == "/codex/usage":
                self.send_error(401)
                return
            if path in ("/claude/usage", "/api/oauth/usage"):
                value = {
                    "five_hour": {"utilization": 38 if separate_usage else 25,
                                  "resets_at": "2030-01-01T00:00:00Z"},
                    "seven_day": {"utilization": 50, "resets_at": "2030-01-05T00:00:00Z"},
                }
                if path == "/api/oauth/usage":
                    available = state.get("reset_offer") == "claude" and not spent.exists()
                    value["cedar_ember"] = {"eligible": True, "next_grant_id": "fixture-grant",
                        "grants": [{"id": "fixture-grant", "resets_left": int(available)}]}
            elif path == "/codex/usage":
                value = {
                    "rate_limit": {
                        "primary_window": {
                            "used_percent": 67 if separate_usage and remote_login else 25,
                            "reset_at": 1893456000,
                            "limit_window_seconds": 18000,
                        },
                        "secondary_window": {
                            "used_percent": 50,
                            "reset_at": 1893801600,
                            "limit_window_seconds": 604800,
                        },
                    },
                    "credits": {"balance": "42"},
                }
            elif path in ("/claude/profile", "/api/oauth/profile"):
                value = {
                    "account": {"uuid": "fixture-account", "email": "fixture@example.invalid"},
                    "organization": {"uuid": CLAUDE_ORG},
                }
            elif path == "/releases/latest":
                if not state.get("version"):
                    self.send_error(404)
                    return
                value = {"tag_name": "v" + state["version"], "draft": False, "prerelease": False}
            elif path.startswith("/releases/download/"):
                relative = Path(path.removeprefix("/releases/download/"))
                release_set = state.get("release_set", "")
                if release_set not in ("", "rollback"):
                    raise ValueError("Unknown fixture release set")
                file = (root / release_set / relative).resolve()
                if not file.is_relative_to(root.resolve()) or not file.is_file():
                    self.send_error(404)
                    return
                data = file.read_bytes()
                if state.get("asset_error") and file.suffix in (".zip", ".gz"):
                    self.send_error(int(state["asset_error"]))
                    return
                if state.get("asset_delay") and file.suffix in (".zip", ".gz"):
                    time.sleep(min(15, state["asset_delay"]))
                if state.get("bad_signature") and file.name == "manifest.sig":
                    data = b"00" * 64
                if state.get("bad_checksum") and file.suffix in (".zip", ".gz"):
                    data = b"x" * len(data)
                self.send_response(200)
                self.send_header("Content-Length", str(len(data)))
                self.end_headers()
                self.wfile.write(data)
                return
            else:
                self.send_error(404)
                return
            data = json.dumps(value).encode()
            self.send_response(200)
            self.send_header("Content-Type", "application/json")
            self.send_header("Content-Length", str(len(data)))
            self.end_headers()
            self.wfile.write(data)

        def do_POST(self):
            if self.path != f"/api/organizations/{CLAUDE_ORG}/reset_rate_limits":
                self.send_error(404)
                return
            if self.headers.get("Authorization") != "Bearer fixture-local-only":
                self.send_error(401)
                return
            body = json.loads(self.rfile.read(int(self.headers["Content-Length"])))
            if (body.get("program"), body.get("grant_id")) != ("cedar_ember", "fixture-grant"):
                self.send_error(400)
                return
            attempt = str(uuid.UUID(body["request_id"]))
            state = json.loads(scenario.read_text())
            if state.get("reset_offer") != "claude":
                self.send_error(409)
                return
            try:
                with spent.open("x") as receipt:
                    receipt.write(attempt)
                result = "reset"
            except FileExistsError:
                result = "already_used"
            with events.open("a") as log:
                log.write(json.dumps({"path": self.path, "grant": body["grant_id"],
                                      "attempt": attempt, "result": result}) + "\n")
            data = json.dumps({"result": result}).encode()
            self.send_response(200)
            self.send_header("Content-Type", "application/json")
            self.send_header("Content-Length", str(len(data)))
            self.end_headers()
            self.wfile.write(data)

    return Handler


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--config", type=Path, required=True)
    parser.add_argument("--scenario", type=Path, required=True)
    parser.add_argument("--releases", type=Path, required=True)
    parser.add_argument("--ready", type=Path, required=True)
    args = parser.parse_args()
    config = load_config(args.config)
    origin = urlsplit(config["origin"])
    server = LoopbackHTTPServer(
        ("127.0.0.1", origin.port),
        handler(args.releases, args.scenario, args.ready.with_suffix(".events")),
    )
    context = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
    context.load_cert_chain(config["certificate"], config["tls_key"])
    server.socket = context.wrap_socket(server.socket, server_side=True)
    args.ready.write_text(config["origin"])
    server.serve_forever()


if __name__ == "__main__":
    main()
