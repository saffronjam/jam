#!/bin/sh
set -eu
# Xvfb hosts KWin's output window. The tested application and input injection
# still use the isolated KWin Wayland session, never the X11 application backend.
export JAM_E2E_HOST_DISPLAY=:99
export DISPLAY=$JAM_E2E_HOST_DISPLAY
export JAM_E2E_NESTED_X11=1
export COMPOSITOR_WIDTH=1280 COMPOSITOR_HEIGHT=800
export JAM_E2E_NATIVE_LAYOUTS=1
private_config=$(mktemp -d "$XDG_RUNTIME_DIR/jam-e2e-config.XXXXXX")
export XDG_CONFIG_HOME=$private_config
cat > "$private_config/kxkbrc" <<'EOF'
[Layout]
Use=true
LayoutList=us,fr,de
VariantList=,,
DisplayNames=,,
EOF
artifacts=${APPIUM_ARTIFACT_OUTPUT_PATH:?Set the private artifact output directory}
mkdir -p "$artifacts"
Xvfb "$DISPLAY" -screen 0 1280x800x24 -nolisten tcp -noreset >"$artifacts/xvfb.log" 2>&1 &
host_pid=$!
trap 'kill "$host_pid" 2>/dev/null || :; wait "$host_pid" 2>/dev/null || :; rm -rf "$private_config"' EXIT
python3 - <<'PY'
import socket
import time
deadline = time.monotonic() + 10
while True:
    try:
        with socket.socket(socket.AF_UNIX) as connection:
            connection.connect('/tmp/.X11-unix/X99')
        break
    except OSError:
        if time.monotonic() >= deadline:
            raise
        time.sleep(.05)
PY
set +e
selenium-webdriver-at-spi-run "$@"
result=$?
exit "$result"
