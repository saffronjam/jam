# Tab creation — 2026-11-19

Run `just benchmark-tabs` on macOS. The benchmark starts isolated temporary daemons for `/bin/sh` and `/bin/zsh`, creates 20 tabs for each, and measures creation and initial snapshot attachment. It then measures 21 offscreen renderer lifetimes, reporting the cold sample separately and the median/p95 for subsequent samples. Temporary daemons and their shells are stopped on completion.

Apple M4, default development build, checked/optimized Ghostty parser:

| Component | Before p95 | After p95 |
| --- | ---: | ---: |
| Warm renderer construction | 60.239 ms | 7.395 ms |
| First frame submission | 19.625 ms | 9.619 ms |

The initial isolated renderer-only run after the change measured 3.991 ms warm construction p95; the table reports the full benchmark run following shell creation instead. These are component timings, not end-to-end GUI latency or a comparison against Ghostty. First-frame submission excludes presentation synchronization.

The complete run measured shell creation (including the durable layout save) at 12.932 ms p95 for sh and 44.594 ms for zsh. Initial snapshot attachment was 0.548 ms and 8.383 ms respectively. Shell configuration and system load affect these values.

Jam now reuses a compatible GPU device/queue and shared font system instead of recreating them for every terminal view. Startup prepares them on the existing background control queue. Each view retains its own surface, atlas, and rendering state, released when evicted from the native view cache. No layout durability guarantees were removed.

A native UI check also created 20 tabs through the shortcut without missing requests. Creation replies compare against the latest applied state, and the tab strip reveals the selected tab.

## Cached switching — 2026-11-20

`just test` includes an AppKit/SwiftUI test that opens three tabs across two workspaces against an isolated daemon, warms their native surfaces, and switches between them 18 times. It verifies renderer identity is preserved, the destination has its expected bounds, terminal rows/columns match those bounds, and a departing host cannot take back a reparented terminal. A development-build run measured 15.3 ms median and 24.4 ms p95 from selection to matching native-view geometry. These timings include run-loop polling; they do not measure compositor presentation or prove a frame-rate guarantee.

Navigation now starts a separate pane-layout identity, so split/close animations do not run across tabs. Cached surfaces attach only after the host has a nonzero size and submit a frame during layout. New surfaces use that actual size and await matching terminal geometry for up to 250 ms; the limit allows viewing a pane whose size is owned by another client. Text always uses the requested cell metrics rather than shrinking a stale grid to fit. Hidden views stop their display links but retain their renderer and terminal mirror.
