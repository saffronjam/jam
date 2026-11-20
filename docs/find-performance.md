# Find in the active tab

The find bar opens immediately; input is coalesced for 40 ms. The main thread only queues requests and polls small summaries. Each visible pane has one background search worker and a one-slot pending queue. New queries cancel superseded scans. Closing the bar cancels all its searches and removes the highlights.

The mirror snapshots its terminal on its own worker, then the search worker decodes and scans every retained row. Unicode grapheme-to-cell mappings preserve match positions across soft wraps and wide characters. Query matching is case-insensitive and literal. The first result and next/previous navigation focus the matching pane and scroll its history into view. Live output is coalesced into refreshes; the spinner appears only after 120 ms. Search does not send terminal input.

On the development Apple M4, a debug-build scan of 9,051 retained rows found 9,050 matches in 191 ms. Cancelling partway through the scan took 1.3 ms in that run. These are component timings, not end-to-end UI guarantees. The regression tests also cover normal and wide Unicode, soft wraps, cancellation, superseded queries, history navigation, and absence of PTY input.
