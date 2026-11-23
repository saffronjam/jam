"""Own native display changes; observe actual pixels, not a replacement renderer."""

import io
import json
import subprocess
from pathlib import Path

from PIL import Image

from .driver import wait


class DisplayScale:
    def __init__(self, ui, surface=("Terminal", "terminal", 0)):
        self.ui = ui
        self.surface = surface
        self.process = None
        self.log = None
        self.original = None
        self.window = None
        self.display_id = None

    def observe(self):
        from .visual import surface_pixels

        png, bounds = surface_pixels(self.ui, *self.surface)
        pixels = Image.open(io.BytesIO(png))
        ratio = pixels.width / bounds["width"]
        if self.ui.platform == "linux":
            native = self.ui.session.execute_script("linux: outputScale", {"inspect": True})
            scale = native["scale"]
        else:
            scale = ratio
        observation = {"action": "display_scale", "scale": scale, "pixel_ratio": ratio,
                       "pixels": list(pixels.size), "bounds": bounds}
        self.ui.observations.append(observation)
        with (self.ui.profile.repo / "scale-observations.jsonl").open("a") as output:
            output.write(json.dumps(observation) + "\n")
        return observation

    def start(self):
        assert self.original is None, "A native display change is already owned"
        original = self.observe()
        assert abs(original["scale"] - 1) < .05, "Use the isolated scale-1 runner for display transitions"
        self.original = original
        self.window = self.ui.window_rect()
        if self.ui.platform == "macos":
            helper = Path(__file__).parents[1] / "native/macos/virtual-display.m"
            binary = self.ui.profile.root / "native-retina-display"
            subprocess.run(["/usr/bin/clang", "-fobjc-arc", "-framework", "AppKit", "-framework",
                            "CoreGraphics", str(helper), "-o", str(binary)],
                           check=True, capture_output=True, text=True, timeout=30)
            ready = self.ui.profile.repo / "scale-display.json"
            self.log = (self.ui.profile.repo / "scale-display.log").open("wb")
            self.process = subprocess.Popen([str(binary), str(ready)], stdout=self.log, stderr=self.log)

            def created():
                assert self.process.poll() is None, "Native Retina display helper exited; inspect scale-display.log"
                return json.loads(ready.read_text()) if ready.exists() else False

            display = wait(created, "owned WindowServer Retina display", timeout=20)
            assert display["pid"] == self.process.pid and display["scale"] == 2
            self.display_id = display["display"]
            self.ui.display_origin = display["rect"]
            current = self.ui.window_rect()
            x, y = display["rect"]["x"] + 20, display["rect"]["y"] + 40
            self.ui._window_drag(current["x"] + current["width"] / 2, current["y"] + 12,
                                 x + current["width"] / 2, y + 12)
            # AppKit adjusts the drag anchor when crossing backing scales.
            # Require the actual window center on the owned output, then
            # independently require a Retina screenshot below.
            actual = self.ui.window_rect()
            rect = display["rect"]
            assert rect["x"] <= actual["x"] + actual["width"] / 2 < rect["x"] + rect["width"]
            assert rect["y"] <= actual["y"] + actual["height"] / 2 < rect["y"] + rect["height"]
        else:
            self.ui.session.execute_script("linux: outputScale", {"scale": 1.25})
            self.ui.capture_scale = 1.25
        wait(lambda: abs(self.observe()["scale"] - (2 if self.ui.platform == "macos" else 1.25)) < .05,
             "real higher-density native terminal output")

    def restore(self):
        if self.original is None:
            return
        try:
            if self.ui.platform == "macos":
                try:
                    if self.ui.display_origin is not None:
                        self.ui.resize(self.window["width"], self.window["height"])
                finally:
                    self.ui.display_origin = None
                    if self.process is not None:
                        if self.process.poll() is None:
                            self.process.terminate()
                        try:
                            self.process.wait(timeout=15)
                        finally:
                            if self.process.poll() is None:
                                self.process.kill()
                                self.process.wait(timeout=5)
                        assert self.process.returncode == 0, "Owned native display cleanup failed"
                    if self.display_id is not None:
                        wait(lambda: self.display_id not in {
                            display["id"] for display in self.ui.session.execute_script(
                                "macos: listDisplays", {}).values()
                        }, "WindowServer removes the exited helper's owned display", timeout=15)
                if self.window is not None:
                    self.ui._move_window(self.window["x"], self.window["y"])
            else:
                self.ui.session.execute_script("linux: outputScale", {"scale": self.original["scale"]})
                self.ui.capture_scale = self.original["scale"]
                if self.window is not None:
                    self.ui.resize(self.window["width"], self.window["height"])
            wait(lambda: abs(self.observe()["scale"] - self.original["scale"]) < .05,
                 "restored native display scale")
        finally:
            if self.log is not None:
                self.log.close()
        self.original = None
