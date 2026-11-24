# Measuring Jam interaction latency

Set `JAM_TRACE_INTERACTIONS=1` when launching the macOS executable and record Points of Interest and Time Profiler in Instruments. The `dev.saffronjam.jam` subsystem records window activation, focus, frame updates, optimistic layout feedback, and layout RPC duration. The trace does not record terminal contents.

On the same Mac and display, capture at least 80 rapid alternating pane clicks and repeat warm tab and workspace switches and pane drags. Repeat with local terminals and a disposable remote with delayed replies and heavy terminal output. Compare local event to visible response independently of request to confirmation and keypress to remote echo. Aim for p95 within one refresh interval and p99 within two for warm interactions.

Use the Dock to bring the same running Jam window forward with a local terminal, quiet remote, busy remote, and disconnected remote selected. Repeat from a minimized window and with a sheet or inline editor open. Record actual clicks and screen updates as well as the in-process trace. App activation and window-key signposts cannot measure the OS interval before Jam receives the event.

For memory, visit the same 16 panes with identical window size, display scale, and terminal workload. Wait at least five seconds after the last switch and compare app footprint using `footprint -p <pid>`. Return to the older panes and compare both the first visible frame and the first live frame. Run `just check` for delayed-response, renderer, and layout regression checks.
