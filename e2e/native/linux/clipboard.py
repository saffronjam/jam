"""Own a native multi-format clipboard on the isolated Wayland desktop."""

import base64
import json
import os
from pathlib import Path
import sys
import threading

import gi

gi.require_version("Gtk", "4.0")
from gi.repository import Gdk, GLib, Gtk

sys.path.insert(0, "/usr/local/share/selenium-webdriver-at-spi")
from gtk_geometry import window_origin

LIMIT = 16 * 1024 * 1024
TITLE = "Jam E2E Clipboard"


def main():
    if not os.environ.get("WAYLAND_DISPLAY") or not os.environ.get("KWIN_PID"):
        raise RuntimeError("Clipboard requires the isolated Wayland desktop")
    Gtk.init()
    display = Gdk.Display.get_default()
    if display is None:
        raise RuntimeError("No native Wayland display")
    clipboard = display.get_clipboard()
    loop = GLib.MainLoop()
    window = Gtk.Window(title=TITLE, default_width=240, default_height=96)
    window.set_child(Gtk.Label(label=TITLE))
    window.present()

    def reply(error=None):
        print(json.dumps({"error": str(error) if error is not None else None}), flush=True)

    def capture(path):
        formats = iter(clipboard.get_formats().get_mime_types())
        items = {}
        remaining = LIMIT

        def next_format():
            mime = next(formats, None)
            if mime is None:
                path.write_text(json.dumps(items))
                reply()
                return
            data = bytearray()

            def on_stream(source, result):
                try:
                    stream, actual = source.read_finish(result)
                    if actual != mime or stream is None:
                        raise RuntimeError(f"Clipboard did not supply {mime}")
                    stream.read_bytes_async(65536, GLib.PRIORITY_DEFAULT, None, on_bytes)
                except Exception as error:
                    reply(error)

            def on_bytes(stream, result):
                nonlocal remaining
                try:
                    chunk = stream.read_bytes_finish(result).get_data()
                    if chunk:
                        remaining -= len(chunk)
                        if remaining < 0:
                            raise ValueError("Clipboard snapshot exceeds 16 MiB")
                        data.extend(chunk)
                        stream.read_bytes_async(65536, GLib.PRIORITY_DEFAULT, None, on_bytes)
                    else:
                        stream.close(None)
                        items[mime] = base64.b64encode(data).decode("ascii")
                        next_format()
                except Exception as error:
                    reply(error)

            clipboard.read_async([mime], GLib.PRIORITY_DEFAULT, None, on_stream)

        next_format()

    def perform(request):
        try:
            window_origin(os.getpid(), TITLE, activate=True)
        except Exception as error:
            reply(error)
            return
        deadline = GLib.get_monotonic_time() + 3_000_000

        def active():
            if not window.is_active():
                if GLib.get_monotonic_time() < deadline:
                    return GLib.SOURCE_CONTINUE
                reply(RuntimeError("Native clipboard window did not receive focus"))
                return GLib.SOURCE_REMOVE
            try:
                path = Path(request["path"])
                if request["action"] == "capture":
                    capture(path)
                elif request["action"] == "restore":
                    items = json.loads(path.read_text())
                    providers = [
                        Gdk.ContentProvider.new_for_bytes(mime, GLib.Bytes.new(base64.b64decode(data)))
                        for mime, data in items.items()
                    ]
                    provider = Gdk.ContentProvider.new_union(providers) if providers else None
                    if not clipboard.set_content(provider):
                        raise RuntimeError("Native clipboard rejected the data")
                    reply()
                else:
                    raise ValueError("Unknown clipboard action")
            except Exception as error:
                reply(error)
            return GLib.SOURCE_REMOVE

        GLib.timeout_add(25, active)

    def input_loop():
        for line in sys.stdin:
            GLib.idle_add(perform, json.loads(line))
        GLib.idle_add(loop.quit)

    threading.Thread(target=input_loop, daemon=True).start()
    reply()
    loop.run()


if __name__ == "__main__":
    main()
