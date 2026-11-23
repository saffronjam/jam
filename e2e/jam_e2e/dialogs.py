"""Read visible desktop dialogs without querying an ambiguous same-bundle helper."""

import csv
import io
import json
import shutil
import subprocess

from PIL import Image

from .driver import wait


def screen_target(ui, text, prefix="dialog", bounds=False, terminal=False, region=None,
                  forbidden=None):
    executable = shutil.which("tesseract")
    assert executable, "Desktop dialog observation requires Tesseract and English language data"
    root = ui.profile.repo

    def locate():
        current_region = region() if callable(region) else region
        if terminal:
            from .visual import terminal_pixels

            png, display = terminal_pixels(ui)
        else:
            png = ui.session.get_screenshot_as_png()
        (root / (prefix + "-screen.png")).write_bytes(png)
        image = Image.open(io.BytesIO(png)).convert("RGB")
        if not terminal:
            if ui.platform == "macos":
                displays = ui.session.execute_script("macos: listDisplays", {})
                assert len(displays) == 1, "Desktop pixel targeting requires its isolated single display"
                display = next(iter(displays.values()))["rect"]
            else:
                display = {"x": 0, "y": 0, "width": image.width / ui.capture_scale,
                           "height": image.height / ui.capture_scale}
        if current_region is not None:
            assert not terminal, "Use one native image region"
            sx, sy = image.width / display["width"], image.height / display["height"]
            box = (round((current_region["x"] - display["x"]) * sx),
                   round((current_region["y"] - display["y"]) * sy),
                   round((current_region["x"] + current_region["width"] - display["x"]) * sx),
                   round((current_region["y"] + current_region["height"] - display["y"]) * sy))
            assert 0 <= box[0] < box[2] <= image.width and 0 <= box[1] < box[3] <= image.height
            image = image.crop(box)
            image.save(root / (prefix + "-window.png"))
            display = current_region
        wanted = text.split()
        matches = []
        enlarged = image.resize((image.width * 2, image.height * 2), Image.Resampling.LANCZOS)
        inputs = [("", enlarged)]
        if current_region is not None:
            # Native default buttons have light text on a saturated blue fill.
            # The ordinary OCR pass misses that text. Also read the SAME pixels
            # with light glyphs made dark, retaining both inputs and results.
            inputs.append(("-light-text", enlarged.convert("L").point(
                lambda value: 0 if value >= 210 else 255)))
        for variant, pixels in inputs:
            stem = prefix + variant + "-ocr"
            pixels.save(root / (stem + ".png"))
            result = subprocess.run(
                [executable, str(root / (stem + ".png")), "stdout", "-l", "eng", "--psm",
                 "6" if terminal else "11", "tsv"],
                check=True, capture_output=True, text=True, timeout=15,
            )
            (root / (stem + ".tsv")).write_text(result.stdout)
            words = [row for row in csv.DictReader(io.StringIO(result.stdout), delimiter="\t")
                     if row["level"] == "5" and row["text"].strip()]
            if forbidden and forbidden in " ".join(word["text"] for word in words):
                return False
            for start in range(len(words) - len(wanted) + 1):
                run = words[start:start + len(wanted)]
                if [word["text"] for word in run] != wanted:
                    continue
                if len({(word["block_num"], word["par_num"], word["line_num"]) for word in run}) != 1:
                    continue
                matches.append(run)
            assert len(matches) <= 1, f"Ambiguous actual screen text {text!r}"
            if matches:
                break
        if not matches:
            return False
        run = matches[0]
        left = min(int(word["left"]) for word in run)
        top = min(int(word["top"]) for word in run)
        right = max(int(word["left"]) + int(word["width"]) for word in run)
        bottom = max(int(word["top"]) + int(word["height"]) for word in run)
        point = {"x": display["x"] + (left + right) / 2 * display["width"] / (image.width * 2),
                 "y": display["y"] + (top + bottom) / 2 * display["height"] / (image.height * 2)}
        rectangle = {"x": display["x"] + left * display["width"] / (image.width * 2),
                     "y": display["y"] + top * display["height"] / (image.height * 2),
                     "width": (right - left) * display["width"] / (image.width * 2),
                     "height": (bottom - top) * display["height"] / (image.height * 2)}
        (root / (prefix + "-target.json")).write_text(json.dumps({"text": text, "point": point,
                                                            "bounds": rectangle, "words": run}, indent=2))
        return rectangle if bounds else point

    return wait(locate, f"actual rendered text {text!r}")
