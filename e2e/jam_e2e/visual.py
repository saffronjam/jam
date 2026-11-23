"""Observe actual terminal pixels independently of accessible text and CLI capture."""

import io
import json
import shutil
import subprocess

from PIL import Image, ImageOps, ImageStat

from .driver import wait


def terminal_pixels(ui):
    return surface_pixels(ui, "Terminal", "terminal", 0)


def surface_pixels(ui, name, role, index=None):
    surface = ui.element(name, role, index=index)
    bounds = surface.rect
    if ui.platform == "macos":
        # XCTest's element screenshot handles native point/pixel scaling.
        return surface.screenshot_as_png, bounds
    # AT-SPI bounds are logical points; the real compositor screenshot contains
    # physical output pixels, including fractional scaling.
    screen = Image.open(io.BytesIO(ui.session.get_screenshot_as_png()))
    scale = ui.capture_scale
    box = (round(bounds["x"] * scale), round(bounds["y"] * scale),
           round((bounds["x"] + bounds["width"]) * scale),
           round((bounds["y"] + bounds["height"]) * scale))
    assert 0 <= box[0] < box[2] <= screen.width and 0 <= box[1] < box[3] <= screen.height
    output = io.BytesIO()
    screen.crop(box).save(output, format="PNG")
    return output.getvalue(), bounds


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
    import shlex
    import sys
    import time
    from pathlib import Path

    ui, root = jam.ui, jam.profile.repo
    path = root / "render-probe.json"
    probe = Path(__file__).with_name("render_probe.py")
    jam.terminal()
    ui.type(shlex.join([sys.executable, str(probe), str(path), mode]))
    ui.key("ENTER")
    wait(path.exists, "synchronized output program startup")
    colors = {"before": (24, 169, 78), "partial": (236, 18, 174), "after": (28, 108, 232)}

    def capture():
        start = time.monotonic_ns()
        png, _ = terminal_pixels(ui)
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
