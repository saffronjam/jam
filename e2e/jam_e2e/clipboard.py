"""Native format transport and deterministic image fixtures."""

import atexit
import base64
import io
import json
from pathlib import Path
import select
import subprocess
import tempfile

from PIL import Image


def image_fixture(kind):
    image = Image.new("RGB", (3, 2))
    image.putdata([
        (241, 0, 12), (0, 234, 8), (25, 14, 255),
        (92, 31, 8), (0, 8, 192), (255, 255, 36),
    ])
    output = io.BytesIO()
    image.save(output, format=kind.upper())
    return output.getvalue(), image.tobytes()


def representations(platform, kind, image, text=None):
    encoded = lambda data: base64.b64encode(data).decode("ascii")
    if platform == "macos":
        fields = [{"type": "public." + kind, "data": encoded(image)}]
        if text is not None:
            fields.append({"type": "public.utf8-plain-text", "data": encoded(text.encode())})
        return [fields]
    fields = {"image/" + kind: encoded(image)}
    if text is not None:
        fields["text/plain;charset=utf-8"] = encoded(text.encode())
    return fields


def verify_image(profile, kind, remote=False):
    import shlex
    from pathlib import PurePosixPath
    from .driver import wait

    def read_path():
        if remote:
            from .remote import peer_command

            return peer_command(profile, "test ! -s /home/fixture/project/image-path.txt || "
                                 "cat /home/fixture/project/image-path.txt")
        receipt = profile.repo / "image-path.txt"
        return receipt.read_text() if receipt.is_file() else ""

    # The trailing shell argument must be parsed separately. This verifies that
    # the inserted path does not consume the keys typed immediately afterward.
    receipt = wait(lambda: (value if (value := read_path()).endswith("\nqueued-after-image\n")
                            else None), "image path and following terminal input", timeout=60)
    path, sentinel = receipt.splitlines()
    assert sentinel == "queued-after-image"
    path = PurePosixPath(path)
    assert path.is_absolute() and path.suffix == ".png" and ".." not in path.parts
    if remote:
        from .remote import peer_command

        assert path.parent.parent == PurePosixPath("/tmp")
        assert path.parent.name.startswith("jam-clipboard-")
        command = ("test -f {path} && test ! -L {path} && "
                   "test \"$(stat -c %a {path})\" = 600 && base64 {path}").format(
                       path=shlex.quote(str(path)))
        image = base64.b64decode(peer_command(profile, command))
        assert not Path(path).exists(), "Remote paste unexpectedly wrote a local image"
    else:
        local = Path(path)
        assert local.parent.resolve().parent == profile.temporary.resolve()
        assert local.parent.name.startswith("jam-clipboard-") and not local.is_symlink()
        assert local.stat().st_mode & 0o777 == 0o600
        image = local.read_bytes()
    exact, pixels = image_fixture(kind)
    if kind == "png":
        assert image == exact, "PNG paste did not preserve exact bytes"
    with Image.open(io.BytesIO(image)) as decoded:
        assert decoded.format == "PNG" and decoded.size == (3, 2)
        assert decoded.convert("RGB").tobytes() == pixels


_linux_owner = None
_linux_log = None


def linux_clipboard(action, profile, data=None):
    # Wayland delivers data from its live owner. Keep the helper alive after
    # each case so restored clipboard contents are still available to the next.
    global _linux_owner, _linux_log
    if _linux_owner is None:
        helper = Path(__file__).parents[1] / "native/linux/clipboard.py"
        # This helper owns the clipboard. The app under test provides the
        # accessibility tree and input targets, so the helper needs no bridge.
        # Only this small GTK owner window uses Cairo. The Jam executable has
        # its own renderer and receives the normal application environment.
        env = dict(profile.env, GDK_BACKEND="wayland", GSK_RENDERER="cairo",
                   GTK_A11Y="none", NO_AT_BRIDGE="1")
        _linux_log = profile.root / "native-clipboard.log"
        with _linux_log.open("w") as diagnostics:
            _linux_owner = subprocess.Popen(
                ["/usr/bin/python3", str(helper)], env=env, text=True,
                stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=diagnostics,
            )
        atexit.register(stop_linux_clipboard)
        read_reply(_linux_owner)
    with tempfile.NamedTemporaryFile(dir=profile.root, mode="w+", encoding="utf-8") as exchange:
        if data is not None:
            json.dump(data, exchange)
            exchange.flush()
        _linux_owner.stdin.write(json.dumps({"action": action, "path": exchange.name}) + "\n")
        _linux_owner.stdin.flush()
        read_reply(_linux_owner)
        if action == "capture":
            return json.load(exchange)
    return None


def read_reply(process):
    if not select.select([process.stdout], [], [], 20)[0]:
        raise TimeoutError("Native Wayland clipboard helper did not respond:\n" +
                           _linux_log.read_text(errors="replace")[-4000:])
    line = process.stdout.readline()
    if not line:
        raise RuntimeError(f"Native Wayland clipboard helper exited: {process.poll()}\n" +
                           _linux_log.read_text(errors="replace")[-4000:])
    error = json.loads(line)["error"]
    if error is not None:
        raise RuntimeError(error)


def stop_linux_clipboard():
    if _linux_owner is None:
        return
    _linux_owner.stdin.close()
    try:
        _linux_owner.wait(timeout=5)
    except subprocess.TimeoutExpired:
        _linux_owner.kill()
        _linux_owner.wait(timeout=5)
    finally:
        _linux_owner.stdout.close()
