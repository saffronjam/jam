# Jam task runner. Run `just` to list every recipe.

set shell := ["bash", "-euo", "pipefail", "-c"]

root := justfile_directory()
export JAM_LAUNCH_DIRECTORY := invocation_directory()
export PATH := root + "/.cache/zig:" + env("HOME") + "/.cargo/bin:" + env("PATH")

[private]
default:
    @just --list --unsorted

[doc('Set up Zig')]
[private]
bootstrap:
    python3 scripts/bootstrap.py

[doc('Build macOS app')]
[group('build')]
build: bootstrap
    cargo build --workspace --locked
    # Make SwiftPM relink Rust changes through its dependency graph.
    touch apps/macos/Sources/CJam/shim.c
    cd apps/macos && swift build
    python3 scripts/package-macos.py

[doc('Build and launch')]
[group('dev')]
dev: build
    open build/Jam.app --args --directory "$JAM_LAUNCH_DIRECTORY"

[doc('Launch UI preview')]
[group('dev')]
preview: build
    open build/Jam.app --args --preview

[doc('Run tests')]
[group('check')]
test mode="all" remote="": bootstrap
    python3 scripts/test.py {{ quote(mode) }} {{ quote(remote) }}

[doc('Lint Rust')]
[group('check')]
lint: bootstrap
    cargo clippy --workspace --all-targets --locked -- -D warnings

[doc('Check formatting')]
[group('check')]
format-check:
    just --fmt --check
    cargo fmt --all -- --check
    if [[ "$(uname -s)" == Darwin ]]; then xcrun swift-format lint --strict apps/macos/Sources/Jam/*.swift apps/macos/Tests/JamTests/*.swift apps/macos/Tests/JamUITests/*.swift apps/macos/Package.swift; fi

[doc('Run all local checks')]
[group('check')]
check: test lint format-check
    if [[ "$(uname -s)" == Darwin ]]; then just build; fi

[doc('Check Linux in Docker')]
[group('check')]
linux-check:
    ./scripts/linux-check

[doc('Measure resize performance')]
[group('check')]
benchmark-resize: bootstrap
    cargo run --locked -p jam-render --example resize_bench

[doc('Measure tab performance')]
[group('check')]
benchmark-tabs: bootstrap
    cargo build --locked -p jam --bin jamd
    cargo run --locked -p jam-render --example tab_bench

[doc('Build Linux servers')]
[group('build')]
remote-build: bootstrap
    mkdir -p build/remote
    cargo run --locked -p jam-core --example snapshot_fixture -- write build/remote/local.snapshot
    ./scripts/remote-build

[doc('Package app and servers')]
[group('build')]
bundle: remote-build build

[doc('Manage local daemon')]
[group('dev')]
daemon action="status":
    #!/usr/bin/env bash
    set -euo pipefail
    action={{ quote(action) }}
    case "$action" in
      start|restart|stop|status) ;;
      *) echo "Usage: just daemon [start|stop|restart|status]" >&2; exit 2 ;;
    esac
    if [[ "$action" == start || "$action" == restart || ! -x target/debug/jam ]]; then
      just bootstrap
      cargo build --locked -p jam
    fi
    python3 scripts/daemon.py "$action"
