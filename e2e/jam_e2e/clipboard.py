"""Native format transport and deterministic image fixtures."""

import atexit
import base64
import io
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


def linux_clipboard(action, profile, data=None):
    """Use the same owned native selection for snapshots and all fixture clipboard writes."""
    global _linux_owner
    with tempfile.TemporaryDirectory(dir=profile.root) as exchange:
        directory = Path(exchange)
        argv = ["/usr/local/bin/jam-e2e-clipboard", action, exchange]
        if action == "capture":
            result = subprocess.run(argv, env=profile.env, text=True, capture_output=True, timeout=20)
            if result.returncode:
                raise AssertionError(result.stderr.strip())
            return {mime: base64.b64encode((directory / str(index)).read_bytes()).decode("ascii")
                    for index, mime in enumerate(result.stdout.splitlines())}
        if action != "restore" or data is None:
            raise ValueError("Expected a clipboard snapshot or a restore with native data")
        for index, encoded in enumerate(data.values()):
            (directory / str(index)).write_bytes(base64.b64decode(encoded, validate=True))
        process = subprocess.Popen(
            [*argv, *data], env=profile.env, text=True, stdin=subprocess.DEVNULL,
            stdout=subprocess.PIPE, stderr=subprocess.PIPE,
        )
        if not select.select([process.stdout], [], [], 15)[0] or process.stdout.readline() != "ready\n":
            diagnostic = stop_linux_owner(process)
            raise AssertionError("The native Wayland clipboard owner did not start. " + diagnostic)
        previous, _linux_owner = _linux_owner, process
        stop_linux_owner(previous)


def stop_linux_owner(process):
    if process is None:
        return ""
    try:
        if process.poll() is None:
            process.terminate()
        process.wait(timeout=5)
    except subprocess.TimeoutExpired:
        process.kill()
        process.wait(timeout=5)
    finally:
        diagnostics = process.stderr.read()
        process.stdout.close()
        process.stderr.close()
    return diagnostics.strip()


@atexit.register
def stop_linux_clipboard():
    stop_linux_owner(_linux_owner)
