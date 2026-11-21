"""Read isolated KWin window positions for GTK's window-relative AT-SPI bounds."""

import json
import os
import tempfile
import time
import uuid

from gi.repository import Gio, GLib


def window_origin(pid, title):
    context = GLib.MainContext.new()
    context.push_thread_default()
    bus = None
    registration = None
    plugin = "jam-e2e-geometry-" + uuid.uuid4().hex
    loaded = False
    try:
        bus = Gio.bus_get_sync(Gio.BusType.SESSION, None)

        def call(path, interface, method, parameters, service="org.kde.KWin"):
            return bus.call_sync(
                service,
                path,
                interface,
                method,
                parameters,
                None,
                Gio.DBusCallFlags.NONE,
                5000,
                None,
            ).unpack()

        owner = call(
            "/org/freedesktop/DBus",
            "org.freedesktop.DBus",
            "GetConnectionUnixProcessID",
            GLib.Variant("(s)", ("org.kde.KWin",)),
            "org.freedesktop.DBus",
        )[0]
        assert owner == int(os.environ["KWIN_PID"]), "Geometry requires the owned compositor"
        received = []
        path = "/dev/saffronjam/E2EGeometry/G" + uuid.uuid4().hex
        interface = "dev.saffronjam.E2EGeometry"
        description = Gio.DBusNodeInfo.new_for_xml(
            f'<node><interface name="{interface}"><method name="Report">'
            '<arg type="s" name="windows" direction="in"/></method></interface></node>'
        )

        def report(connection, sender, object_path, interface_name, method, parameters, invocation):
            received.append(json.loads(parameters.unpack()[0]))
            invocation.return_value(None)

        registration = bus.register_object(path, description.interfaces[0], report, None, None)
        script = (
            "var result = [];\n"
            "for (var window of workspace.windowList()) {\n"
            f"  if (window.pid === {int(pid)} && window.captionNormal === {json.dumps(title)}) {{\n"
            "    var g = window.clientGeometry;\n"
            "    result.push({x:g.x,y:g.y,width:g.width,height:g.height});\n"
            "  }\n}\n"
            f"callDBus({json.dumps(bus.get_unique_name())}, {json.dumps(path)}, "
            f"{json.dumps(interface)}, 'Report', JSON.stringify(result));\n"
        )
        with tempfile.NamedTemporaryFile(mode="w", suffix=".js", prefix=plugin) as source:
            source.write(script)
            source.flush()
            identifier = call(
                "/Scripting",
                "org.kde.kwin.Scripting",
                "loadScript",
                GLib.Variant("(ss)", (source.name, plugin)),
            )[0]
            assert identifier >= 0, "Could not load read-only compositor geometry script"
            loaded = True
            call(f"/Scripting/Script{identifier}", "org.kde.kwin.Script", "run", None)
            deadline = time.monotonic() + 5
            while not received:
                while context.pending():
                    context.iteration(False)
                if time.monotonic() >= deadline:
                    raise AssertionError("Compositor did not report the native window geometry")
                time.sleep(0.005)
        assert len(received) == 1 and len(received[0]) == 1, (
            f"Expected exactly one native window for PID {pid}, title {title!r}: {received}"
        )
        return received[0][0]
    finally:
        try:
            if loaded:
                call(
                    "/Scripting",
                    "org.kde.kwin.Scripting",
                    "unloadScript",
                    GLib.Variant("(s)", (plugin,)),
                )
        finally:
            if registration is not None:
                bus.unregister_object(registration)
            context.pop_thread_default()
