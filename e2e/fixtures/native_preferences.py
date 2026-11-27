"""Configure native toolkit preferences inside the owned fixture profile."""

import os
import sys
from pathlib import Path


def reduced_motion(root, environment):
    root = Path(root).resolve(strict=True)
    configuration = Path(environment["XDG_CONFIG_HOME"]).resolve()
    assert configuration.is_relative_to(root), "Native preferences require an owned configuration directory"
    target = configuration / "gtk-4.0/settings.ini"
    assert target.resolve().is_relative_to(root), "Native preferences cannot follow an external symlink"
    assert not target.exists(), "Do not replace an existing native settings file"
    target.parent.mkdir(parents=True, exist_ok=True)
    target.write_text("[Settings]\ngtk-enable-animations=false\n")


if __name__ == "__main__":
    # macOS supplies its actual NSWorkspace accessibility setting. The case
    # observes it and fails if the native session was not configured for it.
    if sys.platform == "linux":
        reduced_motion(sys.argv[1], os.environ)
