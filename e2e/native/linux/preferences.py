"""Read the real GTK setting through the same Wayland/profile environment."""

import json

import gi

gi.require_version("Gtk", "4.0")
from gi.repository import Gtk

Gtk.init()
settings = Gtk.Settings.get_default()
assert settings is not None, "Native GTK settings require the actual graphical session"
print(json.dumps({"source": "Gtk.Settings.gtk-enable-animations",
                  "reduced_motion": not settings.get_property("gtk-enable-animations")}))
