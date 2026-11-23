"""An automation-tool probe, never an application acceptance substitute."""

import json

import gi

gi.require_version("Gtk", "4.0")
from gi.repository import Gio, Gtk


class Probe(Gtk.Application):
    def __init__(self):
        super().__init__(application_id="dev.saffronjam.E2EDriverProbe",
                         flags=Gio.ApplicationFlags.NON_UNIQUE)
        self.count = 0

    def do_activate(self):
        window = Gtk.ApplicationWindow(application=self, title="Native driver probe")
        box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=12)
        box.set_margin_top(24)
        box.set_margin_bottom(24)
        box.set_margin_start(24)
        box.set_margin_end(24)
        button = Gtk.Button(label="Increment")
        label = Gtk.Label(label="Count: 0")

        def increment(_):
            self.count += 1
            label.set_label(f"Count: {self.count}")

        button.connect("clicked", increment)
        field = Gtk.Entry()
        field.update_property([Gtk.AccessibleProperty.LABEL], ["Probe text"])
        controls = Gtk.Box(orientation=Gtk.Orientation.HORIZONTAL, spacing=12)
        controls.append(button)
        disabled = Gtk.Button(label="Disabled action")
        disabled.set_sensitive(False)
        controls.append(disabled)
        box.append(controls)
        box.append(label)
        box.append(field)
        slider = Gtk.Scale.new_with_range(Gtk.Orientation.HORIZONTAL, 0, 100, 1)
        slider.set_draw_value(False)
        slider.set_value(50)
        slider.update_property([Gtk.AccessibleProperty.LABEL], ["Probe slider"])
        position = Gtk.Label(label="Slider: 50")
        slider.connect(
            "value-changed", lambda widget: position.set_label(f"Slider: {int(widget.get_value())}")
        )
        box.append(slider)
        box.append(position)
        scroll = Gtk.ScrolledWindow()
        scroll.set_size_request(-1, 140)
        adjustment = scroll.get_vadjustment()

        def report_scroll(value):
            print(json.dumps({"scroll": value.get_value(), "upper": value.get_upper(),
                              "page_size": value.get_page_size()}), flush=True)

        adjustment.connect("changed", report_scroll)
        adjustment.connect("value-changed", report_scroll)

        entries = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=12)
        for index in range(8):
            entry = Gtk.Entry()
            entry_label = "Bottom field" if index == 7 else f"Scroll field {index}"
            entry.update_property([Gtk.AccessibleProperty.LABEL], [entry_label])
            entries.append(entry)
        scroll.set_child(entries)
        box.append(scroll)
        window.set_child(box)
        window.set_default_size(600, 600)
        window.present()


Probe().run()
