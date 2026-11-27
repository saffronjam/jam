"""Drive terminal file links and observe the drawer through the native desktop."""

import io
import json
import shlex
import shutil
from pathlib import Path

from PIL import Image, ImageChops, ImageFilter
from selenium.webdriver.common.keys import Keys

from .dialogs import screen_target
from .driver import wait
from .remote import peer_command


def open_file(jam, recipe, remote=False):
    profile, ui = jam.profile, jam.ui
    filename = {
        "text": "./notes.txt", "line": "notes.md:2:4", "markdown": "notes.md",
        "large": "large.md", "image": "image.png", "movie": "clip.mp4", "quicktime": "clip.mov",
        "quoted": '"sample notes.txt"',
    }[recipe]
    if remote:
        assert recipe == "text"
        # A same-named local file makes wrong-host resolution observable.
        (profile.repo / filename).write_text("Local-only preview content\n")
        peer_command(profile, "cd project && printf 'Remote-only preview content\\n' > notes.txt")
    elif recipe in {"text", "quoted"}:
        (profile.repo / filename.strip('"')).write_text("Private drawer content\n")
    elif recipe in {"markdown", "line"}:
        (profile.repo / "notes.md").write_text(
            "# Fixture heading\n**Source marker**\n\n| Field | Value |\n|---|---|\n| Fruit | Pear |\n"
            "\n```mermaid\ngraph LR\nNorth --> South\n```\n"
        )
    elif recipe == "large":
        (profile.repo / filename).write_bytes(b"First page\n" + b"other line\n" * 200_000 + b"Last page\n")
    elif recipe == "image":
        Image.new("RGB", (180, 100), "#fb00fb").save(profile.repo / filename)
    else:
        media = Path(__file__).parents[1] / "fixtures/media" / ("progress" + Path(filename).suffix)
        shutil.copyfile(media, profile.repo / filename)

    # The printed anchor has a fixed number of character cells. Clearing the
    # terminal removes any duplicate appearing in echoed input on a remote PTY.
    command = "printf '\\033[2J\\033[H%s\\n' " + shlex.quote("PREVIEW   " + filename)
    if remote:
        jam.remote_shell(command)
    else:
        jam.run(command)
    anchor = screen_target(ui, "PREVIEW", prefix="file-link", bounds=True, terminal=True)
    x = round(anchor["x"] + anchor["width"] * 12 / 7)
    y = round(anchor["y"] + anchor["height"] / 2)
    (profile.repo / "file-link-click.json").write_text(json.dumps({"x": x, "y": y, "file": filename}))
    if ui.platform == "macos":
        ui.activate_owned_client(inspect=True)
        # XCTest applies the Control modifier to the same native click event.
        ui.session.execute_script("macos: click", {"x": x, "y": y, "keyModifierFlags": 1 << 2})
    else:
        actions = ui._actions(desktop=True)
        actions.pointer_action.move_to_location(x, y).click()
        actions.key_action.key_down(Keys.CONTROL).pause().pause().key_up(Keys.CONTROL)
        actions.perform()


def preview_pixels(jam, color, reveal_controls=False):
    red, green = {"magenta": (255, 0), "cyan": (0, 255)}[color]
    ui = jam.ui

    def frame():
        png = ui.session.get_screenshot_as_png()
        image = Image.open(io.BytesIO(png)).convert("RGB")
        small = image.resize((image.width // 4, image.height // 4), Image.Resampling.NEAREST)
        r, g, b = small.split()
        # ColorSync and YUV decoding shift the dark channel of the movie's
        # magenta frame to about 52 on a real macOS display.
        mask = ImageChops.darker(r.point(lambda v: 255 if abs(v - red) < 75 else 0),
                                g.point(lambda v: 255 if abs(v - green) < 75 else 0))
        mask = ImageChops.darker(mask, b.point(lambda v: 255 if v > 205 else 0))
        # A solid movie or image occupies a region larger than a dock icon.
        mask = mask.filter(ImageFilter.MinFilter(13))
        if mask.histogram()[255] < 100:
            return None
        (jam.profile.repo / f"preview-frame-{color}.png").write_bytes(png)
        left, top, right, bottom = mask.getbbox()
        x, y = (left + right) / 2, (top + bottom) / 2
        return x * 4, y * 4, image.size

    x, y, size = wait(frame, f"{color} preview frame", timeout=25)
    if reveal_controls:
        if ui.platform == "macos":
            displays = ui.session.execute_script("macos: listDisplays", {})
            (display,) = displays.values()
            bounds = display["rect"]
            x = bounds["x"] + x * bounds["width"] / size[0]
            y = bounds["y"] + y * bounds["height"] / size[1]
        else:
            x, y = x / ui.capture_scale, y / ui.capture_scale
        actions = ui._actions(desktop=True)
        actions.pointer_action.move_to_location(round(x), round(y))
        actions.perform()


def set_playback(ui, playing):
    if ui.platform == "macos":
        # AVKit exposes one checkbox for both actions.
        def checked():
            return ui.element("play/pause", "toggle").get_attribute("value") == "1"

        assert checked() != playing, "The player was already in the requested state"
        ui.click("play/pause", "toggle")
        wait(lambda: checked() == playing, "native playback control changed state")
    else:
        ui.click("Play" if playing else "Pause", "button")
        ui.element("Pause" if playing else "Play", "button")


def check_integrations(ui, expected):
    from xml.etree import ElementTree

    def ready():
        root = ElementTree.fromstring(ui.tree())
        rows = [node for node in root.iter()
                if expected in (node.get("name"), node.get("label"), node.get("value"),
                                node.get("description"))]
        if ui.platform == "macos":
            rows = [node for node in rows if float(node.get("width", 0)) > 0
                    and float(node.get("height", 0)) > 0]
        else:
            rows = [node for node in rows if "showing" in node.get("states", "").lower()]
        return len(rows) == 6

    wait(ready, f"all six preview integrations report {expected}")
