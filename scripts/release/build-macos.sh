#!/bin/bash
set -euo pipefail
cd "$(dirname "$0")/../.."
export MACOSX_DEPLOYMENT_TARGET=14.0
python3 scripts/bootstrap.py
export PATH="$PWD/.cache/zig:$HOME/.cargo/bin:$PATH"
cargo build --release --workspace --locked
export JAM_NATIVE_LIB_DIR="$PWD/target/release"
# SwiftPM does not track the external Rust static library.
touch apps/macos/Sources/CJam/shim.c
(cd apps/macos && swift build -c release)
# Verify that both Linux snapshot formats are readable by this exact build.
for fixture in build/remote/*.snapshot; do
  cargo run --release --locked -p jam-core --example snapshot_fixture -- verify "$fixture"
done
python3 scripts/release/notices.py
python3 scripts/package-macos.py --release
mkdir -p build/release
version=$(python3 -c 'import tomllib; print(tomllib.load(open("Cargo.toml", "rb"))["workspace"]["package"]["version"])')
ditto -c -k --sequesterRsrc --keepParent build/packaging/Jam.app "build/release/Jam-v${version}-macos-arm64.zip"
