"""A failed native window move must still stop its owned display service."""

import subprocess
import sys
from types import SimpleNamespace
from unittest.mock import Mock

import pytest

from jam_e2e.scaling import DisplayScale


def test_display_service_stops_even_if_returning_the_window_fails(tmp_path):
    child = subprocess.Popen(
        [sys.executable, "-u", "-c",
         "import signal,sys,time; signal.signal(signal.SIGTERM,lambda *_:sys.exit(0)); "
         "print('ready',flush=True); time.sleep(30)"],
        stdout=subprocess.PIPE, text=True,
    )
    ui = Mock(platform="macos", profile=SimpleNamespace(repo=tmp_path),
              display_origin={"x": 1000, "y": 0})
    ui._move_window.side_effect = RuntimeError("Native window move failed")
    state = DisplayScale(ui)
    state.original = {"scale": 1}
    state.window = {"x": 20, "y": 40, "width": 600, "height": 400}
    state.process = child
    state.log = (tmp_path / "native-service.log").open("wb")
    try:
        assert child.stdout.readline().strip() == "ready"
        with pytest.raises(RuntimeError, match="Native window move failed"):
            state.restore()
        assert child.poll() == 0
        assert ui.display_origin is None
        assert state.log.closed
    finally:
        if child.poll() is None:
            child.kill()
            child.wait(timeout=5)
        child.stdout.close()
