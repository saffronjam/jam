# 6. Separate live persistence from saved layout

Status: Accepted, 2026-11-19

Keep terminal state and bounded scrollback in the daemon's memory. Save workspace names, folder paths, tabs, split trees and proportions atomically. Restore stopped panes after a daemon restart, with an explicit Start Shell action.

GUI reconnection preserves running processes and scrollback. Daemon restart does not. Disk scrollback, crash process recovery, and agent resume behavior are later work. Avoid silently executing stale saved commands.
