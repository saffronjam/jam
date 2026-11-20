# 4. Binary snapshots followed by ordered byte events

Status: Accepted, 2026-11-19

Attach captures libghostty-vt's binary snapshot at a sequence boundary, then streams output bytes and geometry changes. Mirrors decode the same engine's state, preserving unfinished escape sequences and modes. Reject incompatible builds; resnapshot after gaps or backpressure.

A text capture or ANSI screen repaint is not a substitute for a complete snapshot. The engine pin is part of wire compatibility. Images remain disabled until snapshot and renderer behavior are implemented together.
