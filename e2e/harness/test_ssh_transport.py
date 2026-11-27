import re
import socket
import socketserver
import subprocess
import sys
import threading
from pathlib import Path

from jam_e2e.driver import wait


class Echo(socketserver.BaseRequestHandler):
    def handle(self):
        while data := self.request.recv(1024):
            self.request.sendall(data)


def test_transport_loss_spares_the_independent_peer_connection(tmp_path):
    def exchange(connection, payload):
        connection.sendall(payload)
        assert connection.recv(len(payload)) == payload

    with socketserver.ThreadingTCPServer(("127.0.0.1", 0), Echo) as peer:
        thread = threading.Thread(target=peer.serve_forever, daemon=True)
        thread.start()
        port = peer.server_address[1]
        ssh = tmp_path / "home/.ssh"
        ssh.mkdir(parents=True)
        config = f"Host jam-fixture\n  HostName 127.0.0.1\n  Port {port}\n"
        known_hosts = f"[127.0.0.1]:{port} fixture-key\n"
        (ssh / "config").write_text(config)
        (ssh / "known_hosts").write_text(known_hosts)
        helper = Path(__file__).parents[1] / "fixtures/ssh_transport.py"
        process = subprocess.Popen([sys.executable, str(helper), str(tmp_path)])
        try:
            wait((tmp_path / "ssh-transport.ready").exists, "transport proxy ready", timeout=5)
            assert (tmp_path / "ssh-healthy.config").read_text() == config
            assert (tmp_path / "ssh-healthy.known_hosts").read_text() == known_hosts
            [bound] = re.findall(r"^  Port (\d+)$", (ssh / "config").read_text(), re.M)
            assert (ssh / "known_hosts").read_text() == f"[127.0.0.1]:{bound} fixture-key\n"
            address = ("127.0.0.1", int(bound))
            with socket.create_connection(address, timeout=2) as client:
                with socket.create_connection(peer.server_address, timeout=2) as observer:
                    exchange(client, b"before loss")
                    cut = tmp_path / "ssh.cut"
                    blocked = tmp_path / "ssh.blocked"
                    cut.touch()
                    wait(blocked.exists, "transport cut acknowledged", timeout=5)
                    try:
                        assert client.recv(1) == b""
                    except ConnectionResetError:
                        pass
                    exchange(observer, b"still connected")
                    cut.unlink()
                    wait(lambda: not blocked.exists(), "transport restored", timeout=5)
                    with socket.create_connection(address, timeout=2) as reconnected:
                        exchange(reconnected, b"after loss")
        finally:
            process.terminate()
            try:
                assert process.wait(timeout=5) == 0
            finally:
                if process.poll() is None:
                    process.kill()
                    process.wait(timeout=5)
                peer.shutdown()
                thread.join(timeout=5)
