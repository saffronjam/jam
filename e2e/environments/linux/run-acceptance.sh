#!/bin/sh
set -eu
# All paths are constructed inside the same isolated desktop as the app, so
# service recipes cannot retain host-only paths or a different TLS loopback.
python3 e2e/fixtures/build_bundle.py .cache/linux-desktop-bundle \
  --trust .cache/e2e-bundle/trust --releases .cache/e2e-bundle/releases \
  --remote-archive .cache/e2e-bundle/releases/v0.0.4/jam-v0.0.4-linux-aarch64.tar.gz \
  --remote-legacy-archive .cache/previous-server/jam-v0.0.4-linux-aarch64.tar.gz \
  --remote-previous-archive .cache/e2e-bundle/releases/previous/jam-previous-linux-aarch64.tar.gz \
  --remote-limited-archive .cache/e2e-bundle/releases/limited/jam-limited-linux-aarch64.tar.gz \
  --remote-next-archive .cache/e2e-bundle/releases/v0.0.5/jam-v0.0.5-linux-aarch64.tar.gz \
  --remote-incompatible-archive .cache/e2e-bundle/releases/incompatible/jam-incompatible-linux-aarch64.tar.gz \
  --vm-image /work/.cache/debian-bookworm-arm64.qcow2 \
  --qemu /usr/bin/qemu-system-aarch64 --qemu-img /usr/bin/qemu-img \
  --vm-firmware /usr/share/edk2/aarch64/QEMU_EFI.fd
probe=0
/opt/atspi/bin/uv run --project e2e --locked pytest -c e2e/pyproject.toml e2e/driver_checks \
  -o cache_dir=/tmp/pytest-driver-cache --artifacts-dir e2e/artifacts/linux-driver \
  --junitxml=e2e/artifacts/linux-driver.xml || probe=$?
acceptance=0
set --
if [ -n "${E2E_SHARD_INDEX:-}" ]; then
  set -- --shard-index "$E2E_SHARD_INDEX" --shard-count 2 \
    --source-revision "$E2E_SOURCE_REVISION" --fixture-producer "$E2E_FIXTURE_PRODUCER"
fi
/opt/atspi/bin/uv run --project e2e --locked pytest -c e2e/pyproject.toml e2e/tests \
  --app /work/build/linux/bin/Jam --platform linux \
  --cli /work/build/fixture-server/jam --daemon /work/build/fixture-server/jamd \
  --fixture-bundle /work/.cache/linux-desktop-bundle/manifest.json \
  "$@" -o cache_dir=/tmp/pytest-acceptance-cache --junitxml=e2e/artifacts/acceptance.xml || acceptance=$?
test "$probe" -eq 0 && test "$acceptance" -eq 0
