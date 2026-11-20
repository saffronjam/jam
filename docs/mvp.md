# MVP implementation and checks

- Rust daemon and CLI on macOS and Linux; real PTY shells, explicit process close, detached GUI lifetime.
- Native SwiftUI/AppKit folder welcome, sidebar, tabs, split cards, drag proportions, renaming, font sizing, clipboard and IME bridge.
- Shared text renderer: per-cell colors/styles, wide characters, shaping/fallback, cursor styles, selection, cached rows, actual GPU readback.
- Snapshot attach/reconnect, sequence-gap recovery, ordered resizing, per-pane GUI size ownership, private sockets and peer-UID checks.
- Saved workspace/tab/pane layout with explicit stopped-shell restart; no command replay.
- Agent CLI and bundled skill; OSC 7501 records with hierarchy, clears, progress, reasons and prompt lifetimes.
- Linux daemon tests and actual X11/Vulkan renderer window in Docker; full GTK app deferred.

Run `just check` and `just linux-check` from a clean checkout. Do not skip GPU checks when no adapter is available; use a suitable Mac or the Linux software renderer. GUI checks: open a folder, type and paste Unicode, use a full-screen editor, split both ways, drag a divider, rename a workspace/tab, quit and reopen while a command runs, then explicitly stop the daemon and verify stopped panes. Check light/dark appearance and IME on a real desktop.

The automated suite covers partial-escape snapshot restoration, alternate screens and Unicode, protocol limits, sequence-gap resnapshot, status record semantics, private runtime directories, process identity across attach/detach, explicit daemon restart, layout pruning, concurrent GUI size ownership, slow subscribers, terminal-query replies, and pixel readback. CI builds the native Mac target; desktop interaction checks are manual.

Next milestones: GTK client, continued remote lifecycle hardening, Kitty graphics and image snapshot/render transport, capability-scoped agent control, MCP, release packaging and signing. These are required follow-up product work, not hidden MVP implementations.

The [first MVP feedback](feedback/2026-11-19-first-mvp.md) records the user's nine UI and interaction requests, original screenshots, initial findings, and a proposed local-polish round before those larger milestones.

## Local workspace iteration

Implemented worktree grouping and asynchronous Git create/open/remove operations, a compact tab layout, command/navigation palette, configurable shortcuts and settings, key release forwarding, per-pane reconnect messages, and display-linked rendering with coalesced resize requests. That iteration introduced v2. The current v5 protocol requires an incompatible local daemon to be explicitly stopped by its owner before upgrading; Jam does not terminate it automatically.

Use `jam worktree list --workspace ID`, `jam worktree create --workspace ID --branch feature/name`, `jam worktree open /absolute/path`, and `jam worktree remove --workspace ID` (`--force` permits loss of local changes). `jam workspace close --workspace ID --group` closes terminal state without deleting Git files. Worktree operation results are returned after the CLI waits for their operation ID.

Settings live at `$XDG_CONFIG_HOME/jam/config.toml` or `~/.config/jam/config.toml`. The native shortcut editor records chords, rejects conflicts, and supports resetting defaults. Git worktree checkout roots default to `~/wt`.

Deferred per user: automatic repository discovery, subdued repository naming and K/M diff counters. “Jam” refers to jelly / `saffronjam`, not music.

See [resize profiling](resize-performance.md) for reproducible before/after component timings and their limits.
