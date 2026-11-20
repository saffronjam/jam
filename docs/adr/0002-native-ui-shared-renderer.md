# 2. Native UI with a shared renderer

Status: Accepted, 2026-11-19

Use SwiftUI/AppKit on macOS, Rust for the daemon and text renderer, and stable libghostty-vt through pinned Rust bindings. Shape text with cosmic-text and draw through glyphon/wgpu. Build our product without forking the Ghostty app.

Native UI owns window behavior and input methods; the renderer owns terminal cell graphics. Linux first validates this same renderer in a minimal native window. A GTK application is later work. We accept responsibility for terminal graphics correctness, fonts and eventual image support.
