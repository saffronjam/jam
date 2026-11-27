"""Read native session settings without consulting the product implementation."""

import json
import subprocess
from pathlib import Path


def require_reduced_motion(ui):
    native = Path(__file__).parents[1] / "native"
    if ui.platform == "macos":
        command = ["/usr/bin/swift", str(native / "macos/preferences.swift")]
        environment = ui.profile.env
    else:
        command = ["/usr/bin/python3", str(native / "linux/preferences.py")]
        environment = dict(ui.profile.env, GDK_BACKEND="wayland")
    result = subprocess.run(command, env=environment, check=True, capture_output=True,
                            text=True, timeout=30)
    observation = json.loads(result.stdout)
    ui.observations.append({"action": "native_reduced_motion", **observation})
    (ui.profile.repo / "motion-observation.json").write_text(json.dumps(observation, indent=2))
    assert observation["reduced_motion"] is True, f"Native reduced motion is not enabled: {observation}"
    return observation
