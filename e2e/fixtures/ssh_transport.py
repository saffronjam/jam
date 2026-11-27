"""Allow a test to interrupt its SSH path while the owned peer stays available."""

import asyncio
import re
import signal
import sys
from pathlib import Path


async def relay(source, destination):
    while data := await source.read(65536):
        destination.write(data)
        await destination.drain()
    if destination.can_write_eof():
        destination.write_eof()


async def serve(root):
    ssh = root / "home/.ssh"
    config = (ssh / "config").read_text()
    known_hosts = (ssh / "known_hosts").read_text()
    [port] = re.findall(r"^  Port (\d+)$", config, re.M)
    assert "  HostName 127.0.0.1\n" in config
    (root / "ssh-healthy.config").write_text(config)
    (root / "ssh-healthy.known_hosts").write_text(known_hosts)
    requested = root / "ssh.cut"
    blocked_receipt = root / "ssh.blocked"
    clients = set()
    blocked = False

    async def connection(reader, writer):
        clients.add(writer)
        peer = None
        try:
            if blocked:
                return
            upstream, peer = await asyncio.open_connection("127.0.0.1", int(port))
            if blocked or writer.is_closing():
                return
            await asyncio.gather(relay(reader, peer), relay(upstream, writer))
        except ConnectionError:
            # A control-requested TCP disconnect is visible to both ends.
            pass
        finally:
            clients.discard(writer)
            writer.close()
            if peer is not None:
                peer.close()

    loop = asyncio.get_running_loop()
    stop = asyncio.Event()
    loop.add_signal_handler(signal.SIGTERM, stop.set)
    async with await asyncio.start_server(connection, "127.0.0.1", 0) as server:
        [listener] = server.sockets
        forwarded = listener.getsockname()[1]
        (ssh / "config").write_text(config.replace(f"  Port {port}\n", f"  Port {forwarded}\n"))
        (ssh / "known_hosts").write_text(known_hosts.replace(f"[127.0.0.1]:{port}",
                                                          f"[127.0.0.1]:{forwarded}"))
        (root / "ssh-transport.ready").touch()
        while not stop.is_set():
            if requested.exists() != blocked:
                blocked = not blocked
                if blocked:
                    for writer in clients.copy():
                        writer.transport.abort()
                    blocked_receipt.touch()
                else:
                    blocked_receipt.unlink()
            await asyncio.sleep(0.05)
        for writer in clients.copy():
            writer.close()


if __name__ == "__main__":
    asyncio.run(serve(Path(sys.argv[1])))
