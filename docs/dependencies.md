# Dependency boundary

| Component | Pin / boundary | License |
| --- | --- | --- |
| Rust | 1.97.1, `rust-toolchain.toml` | Apache-2.0 / MIT |
| Zig | 0.17.0, verified SHA-256 in `scripts/bootstrap.py` | MIT |
| libghostty-vt Rust bindings | `Uzaaft/libghostty-rs` commit `483ea3d5247b214060da58ef6037a95f30ae89fb` | MIT |
| Ghostty VT | bindings build commit `108cacb17ed8990840cd14abfce43d8ed8503ca1` (1.0.1); graphics disabled | MIT |
| wgpu | 30.0.2, direct dependency | Apache-2.0 / MIT |
| glyphon | 0.12.0, direct dependency | Apache-2.0 / MIT / Zlib |
| cosmic-text | 0.19.0 through glyphon, Cargo.lock | Apache-2.0 / MIT |
| JetBrains Mono | bundled regular/bold/italic/bold-italic, `assets/fonts` | SIL Open Font License, bundled OFL.txt |
| rusqlite / SQLite | rusqlite 0.38, bundled SQLite; durable coordination journal | MIT / public domain |
| ureq / rustls | ureq 3.5.2, TLS versions in Cargo.lock; usage and authenticated release downloads | MIT / Apache-2.0 / ISC |
| ed25519-dalek | Cargo.lock; verifies release manifests against the embedded public key | BSD-3-Clause |
| Rust standard library | Pinned rustup distribution; upstream runtime copyright inventory and license texts bundled | MIT / Apache-2.0 / LLVM exception |
| sha2 / base64 / time | Cargo.lock; opaque account keys, token claims, reset timestamps | MIT / Apache-2.0 |
| core-foundation / security-framework-sys | Cargo.lock, macOS only; native Keychain reads | MIT / Apache-2.0 |

All Rust transitive versions are recorded in Cargo.lock. Building the bindings fetches Ghostty's pinned source into the Cargo build cache. There is no Ghostty app submodule or patched Ghostty renderer. Only `jam-core` uses terminal APIs; only `jam-render` implements terminal graphics; `jam-native` exposes the small C bridge used by Swift.

Use official rustup builds for macOS deployment compatibility. Some package-manager Rust distributions build their standard library for the host OS, defeating an older deployment target. The justfile puts rustup's shims before package-manager binaries.

Primary source references: [Ghostty library](https://github.com/ghostty-org/ghostty/tree/108cacb17ed8990840cd14abfce43d8ed8503ca1), [bindings](https://github.com/Uzaaft/libghostty-rs/tree/483ea3d5247b214060da58ef6037a95f30ae89fb), [glyphon](https://github.com/grovesNL/glyphon), [cosmic-text](https://github.com/pop-os/cosmic-text), [wgpu](https://github.com/gfx-rs/wgpu), [JetBrains Mono](https://github.com/JetBrains/JetBrainsMono).

Release builds run `scripts/release/notices.py`, which follows normal/build dependency edges for the shipped macOS application and Linux servers. It collects the original license texts from locked Cargo sources, pinned upstream copies in `assets/third-party`, and the built Ghostty/Zig sources, including bundled SQLite and artwork/font notices. Missing license text fails the release build. The resulting `build/DEPENDENCIES.txt` is included in the app and Linux archives. Vendored license provenance and SHA-256 checksums are recorded in `assets/third-party/index.json`.
