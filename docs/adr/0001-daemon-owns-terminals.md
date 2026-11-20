# 1. Daemon owns terminals

Status: Accepted, 2026-11-19

Jam's daemon owns PTYs, child supervision, and authoritative terminal state. GUI and CLI are clients. Closing the GUI detaches; closing a pane explicitly terminates it. Layout restoration never replays commands after daemon restart.

This makes sessions independent of GUI lifetime and leaves a clear boundary for remote clients. It does not provide process survival across daemon crashes or reboots.
