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
