# Resize profiling — 2026-11-19

Run `just benchmark-resize`. This is a reproducible offscreen component benchmark, not a window-animation FPS test or a direct comparison with Ghostty.

Reference machine: Apple M4, macOS; default Cargo development profile with libghostty-vt in ReleaseSafe mode. Each run uses 60 size changes per scenario, 70–89 columns and 24–28 rows, with fixed 800×500 render surfaces. Text is populated before the run. Timed work includes terminal resize/reflow, owned frame extraction and GPU submission. GPU readback synchronizes between samples but is excluded from the timing.

| Scenario | Before, median / p95 | After, median / p95 |
| --- | --- | --- |
| Two panes | 42.999 / 52.537 ms | 3.682 / 4.256 ms |
| Four panes | 85.082 / 102.774 ms | 7.371 / 8.147 ms |

The measured change preserves glyph caches when resizing only adds/removes trailing blank cells. Backgrounds and decorations still render across the full grid. A pixel regression test checks that adding blank columns preserves existing wide-character glyph pixels.

The separate frame-copy measurement showed a 0.258 ms p95 deep-copy cost for a 120×40 frame; sharing immutable frames makes that copy unnecessary. Native rendering now follows the view's display link instead of a fixed 30 Hz timer, and pending resize requests are coalesced independently of terminal output parsing.

Live window surface reconfiguration, heavy concurrent output and direct Ghostty comparisons still need broader performance coverage. Do not interpret this table as a guarantee of a specific frame rate on other machines.
