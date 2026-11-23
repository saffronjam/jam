"""Observe actual terminal pixels independently of accessible text and CLI capture."""

import io
import json
import shutil
import subprocess
import shlex
import sys
import time
from pathlib import Path

from PIL import Image, ImageChops, ImageOps, ImageStat

from .driver import wait


def terminal_pixels(ui):
    return surface_pixels(ui, "Terminal", "terminal", 0)


def surface_pixels(ui, name, role, index=None):
    capture, bounds = surface_sampler(ui, name, role, index)
    return capture(), bounds


def surface_sampler(ui, name, role, index=None):
    surface = ui.element(name, role, index=index)
    bounds = surface.rect
    if ui.platform == "macos":
        # XCTest's element screenshot handles native point/pixel scaling.
        return lambda: surface.screenshot_as_png, bounds
    # AT-SPI bounds are logical points; the real compositor screenshot contains
    # physical output pixels, including fractional scaling.
    scale = ui.capture_scale
    box = (round(bounds["x"] * scale), round(bounds["y"] * scale),
           round((bounds["x"] + bounds["width"]) * scale),
           round((bounds["y"] + bounds["height"]) * scale))

    def crop():
        screen = Image.open(io.BytesIO(ui.session.get_screenshot_as_png()))
        assert 0 <= box[0] < box[2] <= screen.width and 0 <= box[1] < box[3] <= screen.height
        output = io.BytesIO()
        screen.crop(box).save(output, format="PNG")
        return output.getvalue()

    return crop, bounds


def terminal_text(ui, expected, appearance=None):
    assert appearance in {None, "light", "dark"}
    executable = shutil.which("tesseract")
    assert executable, "Visual acceptance requires Tesseract OCR and its English language data"
    root = ui.profile.repo
    prefix = f"visual-{len(list(root.glob('visual-*-observation.json'))) + 1:03}"
    version = subprocess.run([executable, "--version"], check=True, capture_output=True,
                             text=True, timeout=10).stdout.splitlines()[0]
    observations = []

    def rendered():
        png, bounds = terminal_pixels(ui)
        (root / (prefix + "-terminal.png")).write_bytes(png)
        image = Image.open(io.BytesIO(png)).convert("RGB")
        assert image.width > 0 and image.height > 0, "Terminal screenshot is empty"
        median = ImageStat.Stat(image.convert("L")).median[0]
        # Preserve the native pixels as evidence, alongside the enlarged OCR
        # input. Enlargement does not supply text from accessibility or state.
        ocr = ImageOps.invert(image) if median < 128 else image
        ocr = ocr.resize((ocr.width * 2, ocr.height * 2), Image.Resampling.LANCZOS)
        # Tesseract needs a border around tightly cropped text. The terminal
        # starts its first glyph at the image edge; native pixels stay intact.
        ocr = ImageOps.expand(ocr, border=20, fill=tuple(ImageStat.Stat(ocr).median))
        ocr.save(root / (prefix + "-ocr.png"))
        result = subprocess.run(
            [executable, str(root / (prefix + "-ocr.png")), "stdout", "-l", "eng", "--psm", "6"],
            check=True, capture_output=True, text=True, timeout=15,
        )
        text = " ".join(result.stdout.split())
        observation = {"bounds": bounds, "pixels": list(image.size), "median_luminance": median,
                       "ocr": text, "stderr": result.stderr}
        observations.append(observation)
        (root / (prefix + "-observation.json")).write_text(json.dumps({
            "engine": version, "expected": expected, "appearance": appearance,
            "observations": observations,
        }, indent=2))
        theme_matches = appearance is None or (median > 128 if appearance == "light" else median < 128)
        return expected in text and theme_matches

    wait(rendered, f"actual terminal pixels show {expected!r} in {appearance or 'current'} appearance")


def synchronized_frames(jam, mode):
    """Sample real terminal images while an independent PTY program holds output."""
    ui, root = jam.ui, jam.profile.repo
    path = root / "render-probe.json"
    probe = Path(__file__).with_name("render_probe.py")
    jam.terminal()
    ui.type(shlex.join([sys.executable, str(probe), path.name, mode]))
    ui.key("ENTER")
    wait(path.exists, "synchronized output program startup")
    take_picture, _ = surface_sampler(ui, "Terminal", "terminal", 0)
    colors = {"before": (24, 169, 78), "partial": (236, 18, 174), "after": (28, 108, 232)}

    def capture():
        start = time.monotonic_ns()
        png = take_picture()
        end = time.monotonic_ns()
        image = Image.open(io.BytesIO(png)).convert("RGB")
        counts = image.getcolors(image.width * image.height)
        labels = [name for name, expected in colors.items()
                  if any(count >= 150 and all(abs(a - b) <= 5 for a, b in zip(rgb, expected))
                         for count, rgb in counts)]
        return dict(start=start, end=end, colors=labels), png

    def initial():
        sample, png = capture()
        if "before" in sample["colors"]:
            (root / "visual-sync-before.png").write_bytes(png)
            return True
        return False

    wait(initial, "initial frame appears in native terminal pixels", timeout=10)
    path.with_suffix(".go").touch()
    wait(lambda: json.loads(path.read_text())["phase"] == "holding",
         "the foreground process has emitted a synchronized partial frame", timeout=5)
    samples = []
    final_color = "after" if mode == "complete" else "partial"

    def finished():
        sample, png = capture()
        samples.append(sample)
        state = json.loads(path.read_text())
        if state["phase"] != "finished" or final_color not in sample["colors"]:
            return False
        (root / "visual-sync-after.png").write_bytes(png)
        return state["intervals"]

    intervals = wait(finished, "completed redraw appears in native pixels", timeout=12)
    inside = [s for s in samples if any(start <= s["start"] <= s["end"] <= end
                                       for start, end in intervals)]
    (root / "visual-sync-timing.json").write_text(json.dumps({
        "mode": mode, "intervals": intervals, "samples": samples,
    }, indent=2))
    assert inside, "No native screenshot was captured entirely within the synchronized update"
    assert all("partial" not in sample["colors"] for sample in inside), (
        "A partial synchronized frame was presented"
    )
    assert any(set(sample["colors"]) & {"before", "after"} for sample in inside)
    ui.type("exit")
    ui.key("ENTER")
    wait(path.with_suffix(".exited").exists, "render program leaves the terminal")


def cursor_frames(jam):
    """Compare terminal surfaces while the foreground program changes only its cursor."""
    ui, root = jam.ui, jam.profile.repo
    path = root / "render-probe.json"
    probe = Path(__file__).with_name("render_probe.py")
    jam.terminal()
    ui.type(shlex.join([sys.executable, str(probe), path.name, "cursor"]))
    ui.key("ENTER")
    take_picture, _ = surface_sampler(ui, "Terminal", "terminal", 0)

    def observe():
        png = take_picture()
        return Image.open(io.BytesIO(png)).convert("RGB")

    def stable(image):
        until = time.monotonic() + 1.2
        while True:
            assert ImageChops.difference(image, observe()).getbbox() is None, (
                "A hidden or steady cursor was redrawn visibly")
            if time.monotonic() >= until:
                return

    def advance(phase):
        path.with_suffix("." + phase).touch()
        wait(lambda: json.loads(path.read_text())["phase"] == phase,
             f"the foreground terminal program emits the {phase} cursor state")

    def blank_frame():
        image = observe()
        # Observe the probe's alternate screen before taking the reference.
        if any(count >= 150 and all(abs(a - b) <= 5 for a, b in zip(rgb, (24, 169, 78)))
               for count, rgb in image.getcolors(image.width * image.height)):
            return image
        return None

    wait(path.exists, "cursor probe starts")
    hidden = wait(blank_frame, "native alternate screen with concealed cursor")
    hidden.save(root / "visual-cursor-hidden.png")
    stable(hidden)
    advance("steady")

    def painted_cursor():
        image = observe()
        return image if ImageChops.difference(hidden, image).getbbox() else None

    steady = wait(painted_cursor, "visible steady cursor pixels")
    x1, y1, x2, y2 = ImageChops.difference(hidden, steady).getbbox()
    assert 0 < x2 - x1 <= 40 and 0 < y2 - y1 <= 65, "Changed pixels extend outside one cursor cell"
    steady.save(root / "visual-cursor-steady.png")
    stable(steady)
    advance("blink")
    wait(lambda: ImageChops.difference(hidden, observe()).getbbox() is None,
         "blinking cursor leaves the hidden reference pixels", timeout=5)
    blink_on = wait(painted_cursor, "blinking cursor reappears", timeout=5)
    blink_on.save(root / "visual-cursor-blink.png")
    advance("concealed")
    wait(lambda: ImageChops.difference(hidden, observe()).getbbox() is None,
         "explicitly hidden cursor returns to the same pixels", timeout=5)
    stable(hidden)
    ui.type("exit")
    ui.key("ENTER")
    wait(path.with_suffix(".exited").exists, "cursor probe leaves the terminal")
