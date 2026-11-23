"""Read or resize owned native windows through the isolated KWin window manager."""

import json
import os
import subprocess
import tempfile
import time
import uuid

from gi.repository import Gio, GLib


def output_scale(scale=None, inspect=False):
    """Read/change the actual output of the owned private Wayland compositor."""
    bus = Gio.bus_get_sync(Gio.BusType.SESSION, None)
    owner = bus.call_sync(
        "org.freedesktop.DBus", "/org/freedesktop/DBus", "org.freedesktop.DBus",
        "GetConnectionUnixProcessID", GLib.Variant("(s)", ("org.kde.KWin",)),
        None, Gio.DBusCallFlags.NONE, 5000, None).unpack()[0]
    assert owner == int(os.environ["KWIN_PID"]) and os.environ.get("WAYLAND_DISPLAY"), (
        "Output scaling requires the owned private Wayland compositor")

    def current():
        result = subprocess.run(["kscreen-doctor", "-j"], check=True, capture_output=True,
                                text=True, timeout=10)
        outputs = [output for output in json.loads(result.stdout)["outputs"]
                   if output["connected"] and output["enabled"]]
        assert len(outputs) == 1, "Fractional scale proof requires one owned output"
        return outputs[0]

    before = current()
    if inspect:
        return before
    assert type(scale) in {int, float} and scale in {1, 1.25}, "Unsupported test output scale"
    subprocess.run(["kscreen-doctor", f"output.{before['id']}.scale.{scale:g}"],
                   check=True, capture_output=True, text=True, timeout=15)
    deadline = time.monotonic() + 10
    while True:
        actual = current()
        assert actual["id"] == before["id"], "The private native output changed identity"
        if abs(actual["scale"] - scale) < .01:
            actual["before_scale"] = before["scale"]
            return actual
        assert time.monotonic() < deadline, "Native Wayland output scale did not change"
        time.sleep(.05)


def keyboard_layout(layout, inspect=False):
    """Select a real layout in the owned compositor, preserving its original ID."""
    bus = Gio.bus_get_sync(Gio.BusType.SESSION, None)

    def call(method, parameters=None):
        return bus.call_sync("org.kde.keyboard", "/Layouts", "org.kde.KeyboardLayouts",
                             method, parameters, None, Gio.DBusCallFlags.NONE, 5000, None).unpack()

    owner = bus.call_sync(
        "org.freedesktop.DBus", "/org/freedesktop/DBus", "org.freedesktop.DBus",
        "GetConnectionUnixProcessID", GLib.Variant("(s)", ("org.kde.keyboard",)),
        None, Gio.DBusCallFlags.NONE, 5000, None).unpack()[0]
    assert owner == int(os.environ["KWIN_PID"]), "Keyboard layout requires the owned private compositor"
    layouts = call("getLayoutsList")[0]
    before = call("getLayout")[0]
    matches = [index for index, item in enumerate(layouts) if item[0] == layout]
    assert len(matches) == 1, f"Native keyboard layout {layout!r} is not uniquely configured: {layouts}"
    observation = {"before": layouts[before][0], "selected": layout, "layouts": layouts}
    if inspect:
        return observation
    call("setLayout", GLib.Variant("(u)", (matches[0],)))
    deadline = time.monotonic() + 5
    while call("getLayout")[0] != matches[0]:
        assert time.monotonic() < deadline, "Native keyboard layout did not change"
        time.sleep(0.05)
    # The pinned synthesizer otherwise falls back to US while KWin uses the
    # selected map. Resolve characters against the VERIFIED compositor layout,
    # then send its actual physical keycodes through the usual Wayland path.
    os.environ["KWIN_XKB_DEFAULT_KEYMAP"] = "true"
    os.environ["XKB_DEFAULT_LAYOUT"] = layout
    observation["synth_layout"] = layout
    return observation


def window_origin(pid, title, rectangle=None, activate=False, allow_unmapped=False):
    assert not allow_unmapped or (not activate and rectangle is None)
    if rectangle is not None:
        assert set(rectangle) == {"x", "y", "width", "height"}
        assert all(type(value) is int for value in rectangle.values())
        assert 0 <= rectangle["x"] < 1280 and 0 <= rectangle["y"] < 800
        assert 320 <= rectangle["width"] <= 1280 and 240 <= rectangle["height"] <= 800
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
            "var matches = workspace.windowList().filter(function(window) {\n"
            f"  return window.pid === {int(pid)} && window.captionNormal === {json.dumps(title)};\n"
            "});\n"
            "if (matches.length === 1) {\n"
            "    var window = matches[0];\n"
            + ("    workspace.activeWindow = window;\n" if activate else "")
            + (
                "    var requested = " + json.dumps(rectangle) + ";\n"
                "    var target = Object.assign({}, window.frameGeometry);\n"
                "    target.x = requested.x; target.y = requested.y;\n"
                "    target.width = requested.width; target.height = requested.height;\n"
                "    window.frameGeometry = target;\n"
                if rectangle is not None else ""
            ) +
            "    var g = window.clientGeometry;\n"
            "    var f = window.frameGeometry;\n"
            "    result.push({x:g.x,y:g.y,width:g.width,height:g.height,\n"
            "      frame:{x:f.x,y:f.y,width:f.width,height:f.height}});\n"
            "}\n"
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
            assert identifier >= 0, "Could not load owned compositor geometry script"
            loaded = True
            call(f"/Scripting/Script{identifier}", "org.kde.kwin.Script", "run", None)
            deadline = time.monotonic() + 5
            while not received:
                while context.pending():
                    context.iteration(False)
                if time.monotonic() >= deadline:
                    raise AssertionError("Compositor did not report the native window geometry")
                time.sleep(0.005)
        if allow_unmapped and received == [[]]:
            return None
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
