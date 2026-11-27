"""Fixture dependencies use actual TLS and detached Ed25519 signatures."""

import json
import socket
import ssl
import subprocess
import threading
from urllib.error import HTTPError, URLError
from urllib.request import Request, urlopen

import pytest

from fixtures.prepare_trust import generate
from fixtures.services import LoopbackHTTPServer, handler, load_config, signed_manifest


def test_https_trust_and_signed_artifacts(tmp_path, monkeypatch):
    def reverse_dns(*args):
        raise AssertionError("Loopback HTTPS startup must not perform reverse DNS")

    monkeypatch.setattr(socket, "getfqdn", reverse_dns)
    generate(tmp_path / "trust", port=1)
    config = load_config(tmp_path / "trust/services.json")
    releases = tmp_path / "releases"
    asset = tmp_path / "application.tar.gz"
    asset.write_bytes(b"fixture application archive")
    release = releases / "v99.0.0"
    signed_manifest(
        release,
        config,
        "99.0.0",
        1,
        [{"path": str(asset), "name": asset.name, "platform": "linux", "arch": "x86_64"}],
    )
    scenario = tmp_path / "world.json"
    scenario.write_text(json.dumps({"version": "99.0.0"}))
    server = LoopbackHTTPServer(("127.0.0.1", 0), handler(releases, scenario, tmp_path / "events"))
    context = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
    context.load_cert_chain(config["certificate"], config["tls_key"])
    server.socket = context.wrap_socket(server.socket, server_side=True)
    thread = threading.Thread(target=server.serve_forever)
    thread.start()
    origin = f"https://localhost:{server.server_port}"
    trusted = ssl.create_default_context(cafile=config["ca_certificate"])
    try:
        with pytest.raises(URLError):
            urlopen(origin + "/releases/latest", timeout=3)
        with urlopen(origin + "/releases/latest", context=trusted, timeout=3) as response:
            assert json.load(response)["tag_name"] == "v99.0.0"
        url = origin + "/releases/download/v99.0.0/manifest.json"
        with urlopen(url, context=trusted, timeout=3) as response:
            downloaded = response.read()
        assert downloaded == (release / "manifest.json").read_bytes()
        public = tmp_path / "public.pem"
        subprocess.run(
            ["openssl", "pkey", "-in", config["signing_key"], "-pubout", "-out", str(public)],
            check=True,
            capture_output=True,
            timeout=5,
        )
        verify = [
            "openssl",
            "pkeyutl",
            "-verify",
            "-rawin",
            "-pubin",
            "-inkey",
            str(public),
            "-in",
            str(release / "manifest.json"),
            "-sigfile",
            str(release / "signature.bin"),
        ]
        subprocess.run(verify, check=True, capture_output=True, timeout=5)
        # Both release sets use the same public version URL. Only the rollback
        # world must receive the separately signed failing candidate.
        broken = tmp_path / "broken-application.tar.gz"
        broken.write_bytes(b"deliberately broken compiled candidate")
        rollback = releases / "rollback/v99.0.0"
        signed_manifest(rollback, config, "99.0.0", 1, [{
            "path": str(broken), "name": asset.name, "platform": "linux", "arch": "x86_64",
        }])
        scenario.write_text(json.dumps({"version": "99.0.0", "release_set": "rollback"}))
        with urlopen(url, context=trusted, timeout=3) as response:
            assert response.read() == (rollback / "manifest.json").read_bytes()
        asset_url = origin + "/releases/download/v99.0.0/" + asset.name
        with urlopen(asset_url, context=trusted, timeout=3) as response:
            assert response.read() == broken.read_bytes()
        scenario.write_text(json.dumps({"version": "99.0.0"}))
        with urlopen(asset_url, context=trusted, timeout=3) as response:
            assert response.read() == asset.read_bytes()
        (release / "manifest.json").write_bytes(downloaded + b" ")
        assert subprocess.run(verify, check=False, capture_output=True, timeout=5).returncode != 0
        scenario.write_text(json.dumps({"reset_offer": "claude"}))
        with urlopen(origin + "/api/oauth/profile", context=trusted, timeout=3) as response:
            org = json.load(response)["organization"]["uuid"]
        claim = Request(origin + f"/api/organizations/{org}/reset_rate_limits",
                        data=json.dumps({"program": "cedar_ember", "grant_id": "fixture-grant",
                                         "request_id": "ffffffff-0000-4100-8200-000000000000"}).encode(),
                        headers={"Authorization": "Bearer fixture-local-only"})
        for expected in ("reset", "already_used"):
            with urlopen(claim, context=trusted, timeout=3) as response:
                assert json.load(response)["result"] == expected
        with urlopen(origin + "/api/oauth/usage?cedar_ember=1", context=trusted, timeout=3) as response:
            assert json.load(response)["cedar_ember"]["grants"][0]["resets_left"] == 0
        scenario.write_text(json.dumps({"http_error": 503}))
        with pytest.raises(HTTPError, match="503"):
            urlopen(origin + "/codex/usage", context=trusted, timeout=3)
    finally:
        server.shutdown()
        thread.join(timeout=5)
        server.server_close()
