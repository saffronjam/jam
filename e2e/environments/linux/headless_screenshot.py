"""Capture KWin's actual nested output from its private Xvfb display."""

import base64
import io
import os

from PIL import ImageGrab


def screenshot():
    display = os.environ["JAM_E2E_HOST_DISPLAY"]
    assert display == ":99" and os.environ.get("KWIN_PID"), "Expected the isolated desktop"
    image = ImageGrab.grab(xdisplay=display)
    assert image.size == (1280, 800), "Unexpected headless output geometry"
    with io.BytesIO() as output:
        image.save(output, format="PNG")
        return base64.b64encode(output.getvalue()).decode("ascii")
