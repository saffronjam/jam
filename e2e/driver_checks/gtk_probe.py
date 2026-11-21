"""An automation-tool probe, never an application acceptance substitute."""

import gi

gi.require_version("Gtk", "4.0")
from gi.repository import Gtk


class Probe(Gtk.Application):
    def __init__(self):
        super().__init__(application_id="dev.saffronjam.E2EDriverProbe")
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
        box.append(button)
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
        window.set_child(box)
        window.set_default_size(600, 400)
        window.present()


Probe().run()
