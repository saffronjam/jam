"""Select/read a real engine on the explicitly owned IBus socket."""

import json
import os
import sys
from pathlib import Path

import gi

gi.require_version("IBus", "1.0")
from gi.repository import IBus

root = Path(os.environ["JAM_E2E_IME_ROOT"]).resolve(strict=True)
assert os.environ["IBUS_ADDRESS"] == "unix:path=" + str(root / "ibus.sock")
assert "IBUS_USE_PORTAL" not in os.environ, "The owned native IME must use its direct socket"
IBus.init()
bus = IBus.Bus.new()
assert bus.is_connected(), "The owned native IBus connection is unavailable"
if len(sys.argv) > 1:
    assert bus.set_global_engine(sys.argv[1]), "The native engine rejected selection"
engine = bus.get_global_engine()
assert engine is not None, "The owned IBus daemon has no selected engine"
print(json.dumps({"selected": engine.get_name(), "address": os.environ["IBUS_ADDRESS"]}))
