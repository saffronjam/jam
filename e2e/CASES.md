# Desktop acceptance cases

298 cases; identical collection on macOS and Linux. No skips or expected failures.

Each numbered step is performed externally. Fixture names identify required setup recipes, not alternate assertions.

## TERM-001: Shell executes a command

Fixture: `isolated local installation`.

1. `('run', "printf 'shell-result'")`
2. `('output', 'shell-result')`

## TERM-002: Shell starts in the opened directory

Fixture: `isolated local installation`.

1. `('run', 'pwd')`
2. `('output_repo',)`

## TERM-003: Unicode and combining text survives the terminal

Fixture: `isolated local installation`.

1. `('run', "printf '\\346\\227\\245\\346\\234\\254\\350\\252\\236\\040\\143\\141\\146\\303\\251\\040\\145\\314\\201\\040\\360\\237\\232\\200'")`
2. `('output', '日本語 café é 🚀')`

## TERM-004: ANSI colors do not corrupt captured text

Fixture: `isolated local installation`.

1. `('run', "printf '\\033[31mred-text\\033[0m'")`
2. `('output', 'red-text')`

## TERM-005: Large output reaches its final line

Fixture: `isolated local installation`.

1. `('run', 'seq 1 10000')`
2. `('output', '10000')`

## TERM-009: Accessible terminal text updates after a shell command

Fixture: `isolated local installation`.

1. `('run', "printf '\\naccessible-output\\n'")`
2. `('visible', 'accessible-output')`

## TERM-006: Terminal geometry reaches the PTY

Fixture: `isolated local installation`.

1. `('run', 'stty size')`
2. `('output_regex', '\\b[1-9]\\d* [1-9]\\d*\\b')`

## TERM-007: Control-C interrupts a running program

Fixture: `isolated local installation`.

1. `('interrupt_probe',)`
2. `('chord', 'ctrl+c')`
3. `('interrupted',)`
4. `('run', "printf 'after-interrupt'")`
5. `('output', 'after-interrupt')`

## TERM-008: Alternate screen exits without losing history

Fixture: `isolated local installation`.

1. `('run', "printf 'before-alt\\n\\033[?1049halternate\\033[?1049lafter-alt'")`
2. `('output', 'after-alt')`
3. `('output', 'before-alt')`

## LAYOUT-001: Native French input preserves text, punctuation and shortcuts

Fixture: `isolated local installation`.

1. `('input_source', 'French')`
2. `('run', "printf '%s' '[azerty] {123}: y-z'")`
3. `('output', '[azerty] {123}: y-z')`
4. `('chord', 'ctrl+shift+t')`
5. `('count', 'tabs', 2)`
6. `('run', 'printf layout-after-shortcut')`
7. `('output', 'layout-after-shortcut')`
8. `('restore_input_source',)`

## LAYOUT-002: Native German input preserves text, punctuation and shortcuts

Fixture: `isolated local installation`.

1. `('input_source', 'German')`
2. `('run', "printf '%s' '[azerty] {123}: y-z'")`
3. `('output', '[azerty] {123}: y-z')`
4. `('chord', 'ctrl+shift+t')`
5. `('count', 'tabs', 2)`
6. `('run', 'printf layout-after-shortcut')`
7. `('output', 'layout-after-shortcut')`
8. `('restore_input_source',)`

## SCALE-001: Native high-density output preserves pixels, hit testing and PTY geometry

Fixture: `isolated local installation`.

1. `('resize', 1000, 660)`
2. `('remember_layout',)`
3. `('remember_pids',)`
4. `('run', "printf '\\116\\101\\124\\111\\126\\105 SCALE 7429'")`
5. `('visual_text', 'NATIVE SCALE 7429')`
6. `('display_scale', 'high')`
7. `('native_scale', 'high')`
8. `('visual_text', 'NATIVE SCALE 7429')`
9. `('geometry_probe',)`
10. `('resize', 900, 580)`
11. `('geometry_changed',)`
12. `('run', "printf '\\123\\103\\101\\114\\105\\104 INPUT 1954'")`
13. `('visual_text', 'SCALED INPUT 1954')`
14. `('same_pids',)`
15. `('same_layout',)`
16. `('display_scale', 'normal')`
17. `('native_scale', 'normal')`
18. `('run', "printf '\\122\\105\\123\\124\\117\\122\\105\\104 SCALE 8263'")`
19. `('visual_text', 'RESTORED SCALE 8263')`

## MOTION-001: Native reduced motion preserves navigation, modal focus and terminal input

Fixture: `native-reduced-motion`.

1. `('native_reduced_motion',)`
2. `('count', 'workspaces', 1)`
3. `('remember_layout',)`
4. `('remember_pids',)`
5. `('chord', 'ctrl+shift+p')`
6. `('target_visible', 'Search commands, workspaces, tabs, and panes', 'field')`
7. `('key', 'ESCAPE')`
8. `('settings', 'Appearance')`
9. `('click', 'Cancel', 'button')`
10. `('run', "printf '\\115\\117\\124\\111\\117\\116 READY 4638'")`
11. `('visual_text', 'MOTION READY 4638')`
12. `('same_layout',)`
13. `('same_pids',)`

## CLIP-001: Clipboard paste preserves 'ascii paste'

Fixture: `isolated local installation`.

1. `('clipboard', 'ascii paste')`
2. `('begin_input', 'cat > pasted.txt')`
3. `('paste',)`
4. `('key', 'ENTER')`
5. `('chord', 'ctrl+d')`
6. `('chord', 'ctrl+d')`
7. `('file_text', 'pasted.txt', 'ascii paste\n')`

## CLIP-002: Clipboard paste preserves '日本語 café'

Fixture: `isolated local installation`.

1. `('clipboard', '日本語 café')`
2. `('begin_input', 'cat > pasted.txt')`
3. `('paste',)`
4. `('key', 'ENTER')`
5. `('chord', 'ctrl+d')`
6. `('chord', 'ctrl+d')`
7. `('file_text', 'pasted.txt', '日本語 café\n')`

## CLIP-003: Clipboard paste preserves 'spaces  and\ttabs'

Fixture: `isolated local installation`.

1. `('clipboard', 'spaces  and\ttabs')`
2. `('begin_input', 'cat > pasted.txt')`
3. `('paste',)`
4. `('key', 'ENTER')`
5. `('chord', 'ctrl+d')`
6. `('chord', 'ctrl+d')`
7. `('file_text', 'pasted.txt', 'spaces  and\ttabs\n')`

## CLIP-004: Clipboard paste preserves 'first\nsecond'

Fixture: `isolated local installation`.

1. `('clipboard', 'first\nsecond')`
2. `('begin_input', 'cat > pasted.txt')`
3. `('paste',)`
4. `('key', 'ENTER')`
5. `('chord', 'ctrl+d')`
6. `('chord', 'ctrl+d')`
7. `('file_text', 'pasted.txt', 'first\nsecond\n')`

## CLIP-005: Clipboard paste preserves 'é 🚀 👨\u200d👩\u200d👧\u200d👦'

Fixture: `isolated local installation`.

1. `('clipboard', 'é 🚀 👨\u200d👩\u200d👧\u200d👦')`
2. `('begin_input', 'cat > pasted.txt')`
3. `('paste',)`
4. `('key', 'ENTER')`
5. `('chord', 'ctrl+d')`
6. `('chord', 'ctrl+d')`
7. `('file_text', 'pasted.txt', 'é 🚀 👨\u200d👩\u200d👧\u200d👦\n')`

## NAV-001: New Tab creates exactly one object

Fixture: `isolated local installation`.

1. `('command', 'New Tab')`
2. `('key', 'ESCAPE')`
3. `('count', 'tabs', 2)`

## NAV-002: Split Right creates exactly one object

Fixture: `isolated local installation`.

1. `('command', 'Split Right')`
2. `('key', 'ESCAPE')`
3. `('count', 'panes', 2)`

## NAV-003: Split Down creates exactly one object

Fixture: `isolated local installation`.

1. `('command', 'Split Down')`
2. `('key', 'ESCAPE')`
3. `('count', 'panes', 2)`

## NAV-004: New Workspace creates exactly one object

Fixture: `isolated local installation`.

1. `('command', 'New Workspace')`
2. `('key', 'ESCAPE')`
3. `('count', 'workspaces', 2)`

## TAB-001: Closing a tab terminates its pane

Fixture: `isolated local installation`.

1. `('command', 'New Tab')`
2. `('key', 'ESCAPE')`
3. `('context', 'Tab 2', 'button')`
4. `('click', 'Close Tab', 'menu')`
5. `('count', 'tabs', 1)`
6. `('count', 'panes', 1)`

## TAB-002: Middle click closes a background tab

Fixture: `isolated local installation`.

1. `('command', 'New Tab')`
2. `('key', 'ESCAPE')`
3. `('middle', 'Tab 1')`
4. `('count', 'tabs', 1)`

## TAB-003: Renamed tab persists after reopening

Fixture: `isolated local installation`.

1. `('clipboard', 'Review 日本語')`
2. `('context', 'Tab 1')`
3. `('click', 'Rename')`
4. `('fill', 'Tab name', 'Review 日本語')`
5. `('field_value', 'Tab name', 'Review 日本語')`
6. `('key', 'ENTER')`
7. `('tab_name', 'Review 日本語')`
8. `('relaunch',)`
9. `('visible', 'Review 日本語')`

## TAB-004: Escape cancels inline rename

Fixture: `isolated local installation`.

1. `('context', 'Tab 1')`
2. `('click', 'Rename')`
3. `('type', 'must-not-save')`
4. `('key', 'ESCAPE')`
5. `('visible', 'Tab 1')`
6. `('absent', 'must-not-save')`

## LIFE-001: Closing and reopening the app retains the shell process

Fixture: `isolated local installation`.

1. `('remember_pids',)`
2. `('relaunch',)`
3. `('same_pids',)`

## LIFE-002: Settings dialogs do not consume terminal input

Fixture: `isolated local installation`.

1. `('settings', 'Appearance')`
2. `('type', 'do-not-send-to-shell')`
3. `('click', 'Cancel')`
4. `('no_output', 'do-not-send-to-shell')`

## LIFE-003: Daemon restart does not replay a command

Fixture: `isolated local installation`.

1. `('run', 'printf x >> executions')`
2. `('restart_server',)`
3. `('file_text', 'executions', 'x', 2)`

## LIFE-004: Cancelled server restart leaves the shell alive

Fixture: `isolated local installation`.

1. `('remember_pids',)`
2. `('context', 'Local')`
3. `('click', 'Restart Server…')`
4. `('click', 'Cancel')`
5. `('same_pids',)`

## CFG-001: Close workspace after its last tab closes: save

Fixture: `isolated local installation`.

1. `('settings', 'General')`
2. `('click', 'Close workspace after its last tab closes', 'toggle')`
3. `('save',)`
4. `('config', 'close_empty_workspaces', True)`

## CFG-002: Close workspace after its last tab closes: cancel

Fixture: `isolated local installation`.

1. `('settings', 'General')`
2. `('click', 'Close workspace after its last tab closes', 'toggle')`
3. `('click', 'Cancel')`
4. `('config_default', 'close_empty_workspaces', False)`

## CFG-003: Close workspace after its last tab closes: reopen

Fixture: `isolated local installation`.

1. `('settings', 'General')`
2. `('click', 'Close workspace after its last tab closes', 'toggle')`
3. `('save',)`
4. `('relaunch',)`
5. `('config', 'close_empty_workspaces', True)`

## CFG-004: Close tab after its last pane closes: save

Fixture: `isolated local installation`.

1. `('settings', 'General')`
2. `('click', 'Close tab after its last pane closes', 'toggle')`
3. `('save',)`
4. `('config', 'close_empty_tabs', False)`

## CFG-005: Close tab after its last pane closes: cancel

Fixture: `isolated local installation`.

1. `('settings', 'General')`
2. `('click', 'Close tab after its last pane closes', 'toggle')`
3. `('click', 'Cancel')`
4. `('config_default', 'close_empty_tabs', True)`

## CFG-006: Close tab after its last pane closes: reopen

Fixture: `isolated local installation`.

1. `('settings', 'General')`
2. `('click', 'Close tab after its last pane closes', 'toggle')`
3. `('save',)`
4. `('relaunch',)`
5. `('config', 'close_empty_tabs', False)`

## CFG-007: Allow deleting checkout files: save

Fixture: `isolated local installation`.

1. `('settings', 'General')`
2. `('click', 'Allow deleting checkout files', 'toggle')`
3. `('save',)`
4. `('config', 'allow_workspace_deletion', False)`

## CFG-008: Allow deleting checkout files: cancel

Fixture: `isolated local installation`.

1. `('settings', 'General')`
2. `('click', 'Allow deleting checkout files', 'toggle')`
3. `('click', 'Cancel')`
4. `('config_default', 'allow_workspace_deletion', True)`

## CFG-009: Allow deleting checkout files: reopen

Fixture: `isolated local installation`.

1. `('settings', 'General')`
2. `('click', 'Allow deleting checkout files', 'toggle')`
3. `('save',)`
4. `('relaunch',)`
5. `('config', 'allow_workspace_deletion', False)`

## CFG-010: Copy text on selection: save

Fixture: `isolated local installation`.

1. `('settings', 'General')`
2. `('click', 'Copy text on selection', 'toggle')`
3. `('save',)`
4. `('config', 'copy_on_select', True)`

## CFG-011: Copy text on selection: cancel

Fixture: `isolated local installation`.

1. `('settings', 'General')`
2. `('click', 'Copy text on selection', 'toggle')`
3. `('click', 'Cancel')`
4. `('config_default', 'copy_on_select', False)`

## CFG-012: Copy text on selection: reopen

Fixture: `isolated local installation`.

1. `('settings', 'General')`
2. `('click', 'Copy text on selection', 'toggle')`
3. `('save',)`
4. `('relaunch',)`
5. `('config', 'copy_on_select', True)`

## CFG-013: Automatically check for updates: save

Fixture: `isolated local installation`.

1. `('settings', 'General')`
2. `('click', 'Automatically check for updates', 'toggle')`
3. `('save',)`
4. `('config', 'automatic_check_updates', False)`

## CFG-014: Automatically check for updates: cancel

Fixture: `isolated local installation`.

1. `('settings', 'General')`
2. `('click', 'Automatically check for updates', 'toggle')`
3. `('click', 'Cancel')`
4. `('config_default', 'automatic_check_updates', True)`

## CFG-015: Automatically check for updates: reopen

Fixture: `isolated local installation`.

1. `('settings', 'General')`
2. `('click', 'Automatically check for updates', 'toggle')`
3. `('save',)`
4. `('relaunch',)`
5. `('config', 'automatic_check_updates', False)`

## CFG-016: Automatically resume interrupted work: save

Fixture: `isolated local installation`.

1. `('settings', 'Coordination')`
2. `('click', 'Automatically resume interrupted work', 'toggle')`
3. `('save',)`
4. `('config', 'resume_interrupted_work', False)`

## CFG-017: Automatically resume interrupted work: cancel

Fixture: `isolated local installation`.

1. `('settings', 'Coordination')`
2. `('click', 'Automatically resume interrupted work', 'toggle')`
3. `('click', 'Cancel')`
4. `('config_default', 'resume_interrupted_work', True)`

## CFG-018: Automatically resume interrupted work: reopen

Fixture: `isolated local installation`.

1. `('settings', 'Coordination')`
2. `('click', 'Automatically resume interrupted work', 'toggle')`
3. `('save',)`
4. `('relaunch',)`
5. `('config', 'resume_interrupted_work', False)`

## CFG-019: Persist Light appearance

Fixture: `isolated local installation`.

1. `('settings', 'Appearance')`
2. `('click', 'Light')`
3. `('save',)`
4. `('config', 'appearance', 'light')`
5. `('relaunch',)`
6. `('config', 'appearance', 'light')`

## CFG-020: Persist Dark appearance

Fixture: `isolated local installation`.

1. `('settings', 'Appearance')`
2. `('click', 'Dark')`
3. `('save',)`
4. `('config', 'appearance', 'dark')`
5. `('relaunch',)`
6. `('config', 'appearance', 'dark')`

## CFG-021: Persist System appearance

Fixture: `isolated local installation`.

1. `('settings', 'Appearance')`
2. `('click', 'System')`
3. `('save',)`
4. `('config', 'appearance', 'system')`
5. `('relaunch',)`
6. `('config', 'appearance', 'system')`

## CFG-022: External edits invalidate an open draft

Fixture: `isolated local installation`.

1. `('settings', 'General')`
2. `('write_config', 'max_agents_per_coordinator = 3\n')`
3. `('visible', 'Settings changed outside this window')`
4. `('disabled', 'Save')`
5. `('click', 'Cancel')`
6. `('config', 'max_agents_per_coordinator', 3)`

## CFG-023: Reopening after a conflict loads the new settings

Fixture: `isolated local installation`.

1. `('settings', 'General')`
2. `('write_config', 'appearance = "dark"\n')`
3. `('visible', 'Settings changed outside this window')`
4. `('click', 'Cancel')`
5. `('settings', 'Appearance')`
6. `('save',)`
7. `('config', 'appearance', 'dark')`

## CFG-024: Malformed configuration produces a visible error (24)

Fixture: `isolated local installation`.

1. `('write_config', 'appearance = "invalid"\n')`
2. `('visible', 'Could not load settings')`
3. `('run', 'printf still-running')`

## CFG-025: Malformed configuration produces a visible error (25)

Fixture: `isolated local installation`.

1. `('write_config', 'unknown_setting = true\n')`
2. `('visible', 'Could not load settings')`
3. `('run', 'printf still-running')`

## CFG-026: Malformed configuration produces a visible error (26)

Fixture: `isolated local installation`.

1. `('write_config', 'max_agents_per_coordinator = 0\n')`
2. `('visible', 'Could not load settings')`
3. `('run', 'printf still-running')`

## CFG-027: Malformed configuration produces a visible error (27)

Fixture: `isolated local installation`.

1. `('write_config', 'enabled_agents = ["unknown"]\n')`
2. `('visible', 'Could not load settings')`
3. `('run', 'printf still-running')`

## CFG-028: Malformed configuration produces a visible error (28)

Fixture: `isolated local installation`.

1. `('write_config', '[bindings]\nunknown_action = ["ctrl+j"]\n')`
2. `('visible', 'Could not load settings')`
3. `('run', 'printf still-running')`

## CFG-029: Malformed configuration produces a visible error (29)

Fixture: `isolated local installation`.

1. `('write_config', '[bindings]\nfind = ["f"]\n')`
2. `('visible', 'Could not load settings')`
3. `('run', 'printf still-running')`

## CFG-030: Malformed configuration produces a visible error (30)

Fixture: `isolated local installation`.

1. `('write_config', '[bindings]\nfind = ["ctrl+j"]\npalette = ["ctrl+j"]\n')`
2. `('visible', 'Could not load settings')`
3. `('run', 'printf still-running')`

## CFG-031: Malformed configuration produces a visible error (31)

Fixture: `isolated local installation`.

1. `('write_config', 'broken = [\n')`
2. `('visible', 'Could not load settings')`
3. `('run', 'printf still-running')`

## CFG-032: Saving through a symlink preserves it

Fixture: `isolated local installation`.

1. `('config_symlink',)`
2. `('settings', 'Appearance')`
3. `('click', 'Dark')`
4. `('save',)`
5. `('config', 'appearance', 'dark')`
6. `('symlink_preserved',)`

## KEY-001: New Tab via configured keyboard chord

Fixture: `isolated local installation`.

1. `('chord', 'ctrl+shift+t')`
2. `('key', 'ESCAPE')`
3. `('count', 'tabs', 2)`

## KEY-002: Split Right via configured keyboard chord

Fixture: `isolated local installation`.

1. `('chord', 'ctrl+shift+e')`
2. `('key', 'ESCAPE')`
3. `('count', 'panes', 2)`

## KEY-003: Split Down via configured keyboard chord

Fixture: `isolated local installation`.

1. `('chord', 'ctrl+shift+o')`
2. `('key', 'ESCAPE')`
3. `('count', 'panes', 2)`

## KEY-004: New Workspace via configured keyboard chord

Fixture: `isolated local installation`.

1. `('chord', 'ctrl+shift+n')`
2. `('key', 'ESCAPE')`
3. `('count', 'workspaces', 2)`

## KEY-005: A custom binding executes its command

Fixture: `isolated local installation`.

1. `('write_config', '[bindings]\nnew_tab = ["ctrl+alt+j"]\n')`
2. `('relaunch',)`
3. `('chord', 'ctrl+alt+j')`
4. `('key', 'ESCAPE')`
5. `('count', 'tabs', 2)`

## KEY-006: An explicitly unbound command stays disabled after restart

Fixture: `isolated local installation`.

1. `('write_config', '[bindings]\nnew_tab = []\n')`
2. `('relaunch',)`
3. `('chord', 'ctrl+shift+t')`
4. `('settle_count', 'tabs', 1)`

## KEY-007: Every configured binding is effective

Fixture: `isolated local installation`.

1. `('write_config', '[bindings]\nnew_tab = ["ctrl+alt+j", "ctrl+alt+k"]\n')`
2. `('relaunch',)`
3. `('chord', 'ctrl+alt+j')`
4. `('target_visible', 'Tab name', 'field')`
5. `('key', 'ESCAPE')`
6. `('count', 'tabs', 2)`
7. `('chord', 'ctrl+alt+k')`
8. `('target_visible', 'Tab name', 'field')`
9. `('key', 'ESCAPE')`
10. `('count', 'tabs', 3)`

## FIND-001: History search finds needle

Fixture: `isolated local installation`.

1. `('run', "printf 'needle\\n'")`
2. `('command', 'Find in Tab History')`
3. `('fill', 'Find in tab history', 'needle')`
4. `('visible', '1/1')`

## FIND-002: History search finds 日本語

Fixture: `isolated local installation`.

1. `('run', "printf '日本語\\n'")`
2. `('command', 'Find in Tab History')`
3. `('fill', 'Find in tab history', '日本語')`
4. `('visible', '1/1')`

## FIND-003: History search finds wrapped

Fixture: `isolated local installation`.

1. `('run', "printf 'wrapped\\n'")`
2. `('command', 'Find in Tab History')`
3. `('fill', 'Find in tab history', 'wrapped')`
4. `('visible', '1/1')`

## FIND-004: History search finds case

Fixture: `isolated local installation`.

1. `('run', "printf 'case\\n'")`
2. `('command', 'Find in Tab History')`
3. `('fill', 'Find in tab history', 'case')`
4. `('visible', '1/1')`

## FIND-005: A search with no matches reports zero

Fixture: `isolated local installation`.

1. `('command', 'Find in Tab History')`
2. `('fill', 'Find in tab history', 'never-produced-59104')`
3. `('visible', '0/0')`

## FIND-006: Escape returns keyboard focus to the terminal

Fixture: `isolated local installation`.

1. `('command', 'Find in Tab History')`
2. `('target_visible', 'Find in tab history', 'field')`
3. `('key', 'ESCAPE')`
4. `('absent', 'Find in tab history')`
5. `('run_focused', 'printf after-find')`
6. `('output', 'after-find')`

## FIND-007: Find follows a query across a soft-wrapped terminal line

Fixture: `isolated local installation`.

1. `('run', 'cols=$(stty size); cols=${cols##* }; printf \'%*s%s\\n\' "$((cols-4))" \'\' \'boundary-search\'')`
2. `('command', 'Find in Tab History')`
3. `('fill', 'Find in tab history', 'boundary-search')`
4. `('visible', '1/1')`

## RACE-001: Rapid history queries settle on the final query without later stale counts

Fixture: `isolated local installation`.

1. `('run', "seq 1 6000 | sed 's/^/racealpha-/'; printf '\\nfinal-racebeta\\n'")`
2. `('remember_layout',)`
3. `('remember_pids',)`
4. `('command', 'Find in Tab History')`
5. `('query_burst', 'Find in tab history', ('racealpha', 'no-match-1973', 'racealpha', 'final-racebeta'))`
6. `('field_value', 'Find in tab history', 'final-racebeta')`
7. `('stable_visible', '1/1', 2)`
8. `('key', 'ESCAPE')`
9. `('absent', 'Find in tab history')`
10. `('run_focused', 'printf search-final-query-focus')`
11. `('output', 'search-final-query-focus')`
12. `('same_layout',)`
13. `('same_pids',)`

## RACE-002: Dismissing rapidly replaced history queries keeps search closed and restores terminal focus

Fixture: `isolated local installation`.

1. `('run', "seq 1 6000 | sed 's/^/cancel-races-/'")`
2. `('remember_layout',)`
3. `('remember_pids',)`
4. `('command', 'Find in Tab History')`
5. `('query_burst', 'Find in tab history', ('cancel-races', 'never-matches-8249', 'cancel-races'))`
6. `('key', 'ESCAPE')`
7. `('absent', 'Find in tab history')`
8. `('run_focused', 'printf cancelled-search-focus')`
9. `('output', 'cancelled-search-focus')`
10. `('absent', 'Find in tab history')`
11. `('same_layout',)`
12. `('same_pids',)`

## RACE-003: History results follow the selected tab after rapid query replacement

Fixture: `isolated local installation`.

1. `('run', 'printf scope-first-tab')`
2. `('command', 'New Tab')`
3. `('key', 'ESCAPE')`
4. `('count', 'tabs', 2)`
5. `('run', 'printf scope-second-tab')`
6. `('remember_layout',)`
7. `('remember_pids',)`
8. `('command', 'Find in Tab History')`
9. `('query_burst', 'Find in tab history', ('scope-first-tab', 'no-match-5917', 'scope-second-tab'))`
10. `('click', 'Tab 1', 'button')`
11. `('field_value', 'Find in tab history', 'scope-second-tab')`
12. `('stable_visible', '0/0', 2)`
13. `('click', 'Tab 2', 'button')`
14. `('stable_visible', '1/1', 2)`
15. `('key', 'ESCAPE')`
16. `('absent', 'Find in tab history')`
17. `('run_focused', 'printf search-scope-focus')`
18. `('output', 'search-scope-focus')`
19. `('same_layout',)`
20. `('same_pids',)`

## WT-001: Create worktree feature/one through the UI

Fixture: `isolated local installation`.

1. `('command', 'New Worktree')`
2. `('fill', 'Branch name', 'feature/one')`
3. `('fill', 'Base revision', 'HEAD')`
4. `('click', 'Create')`
5. `('count', 'workspaces', 2)`
6. `('git_branch', 'feature/one')`

## WT-002: Create worktree 日本語 through the UI

Fixture: `isolated local installation`.

1. `('command', 'New Worktree')`
2. `('fill', 'Branch name', '日本語')`
3. `('fill', 'Base revision', 'HEAD')`
4. `('click', 'Create')`
5. `('count', 'workspaces', 2)`
6. `('git_branch', '日本語')`

## WT-003: Create worktree with-dashes through the UI

Fixture: `isolated local installation`.

1. `('command', 'New Worktree')`
2. `('fill', 'Branch name', 'with-dashes')`
3. `('fill', 'Base revision', 'main')`
4. `('click', 'Create')`
5. `('count', 'workspaces', 2)`
6. `('git_branch', 'with-dashes')`

## WT-004: Cancelling worktree creation preserves the repository

Fixture: `isolated local installation`.

1. `('command', 'New Worktree')`
2. `('fill', 'Branch name', 'cancelled')`
3. `('click', 'Cancel')`
4. `('settle_count', 'workspaces', 1)`
5. `('git_no_branch', 'cancelled')`

## WT-005: Invalid base revision produces a recoverable error

Fixture: `isolated local installation`.

1. `('remember_layout',)`
2. `('remember_pids',)`
3. `('command', 'New Worktree')`
4. `('fill', 'Branch name', 'bad-base')`
5. `('fill', 'Base revision', 'no-such-ref')`
6. `('click', 'Create')`
7. `('click', 'Details', 'button')`
8. `('visible', 'no-such-ref')`
9. `('count', 'workspaces', 1)`
10. `('same_layout',)`
11. `('same_pids',)`
12. `('git_no_branch', 'bad-base')`

## AGENT-001: provider missing

Fixture: `provider-missing`.

1. `('command', 'Agent Integrations')`
2. `('visible', 'Not detected')`

## AGENT-002: provider detected

Fixture: `provider-detected`.

1. `('command', 'Agent Integrations')`
2. `('visible', 'Install')`

## AGENT-003: provider installed

Fixture: `provider-installed`.

1. `('command', 'Agent Integrations')`
2. `('visible', 'Installed')`

## AGENT-004: provider foreign

Fixture: `provider-foreign`.

1. `('command', 'Agent Integrations')`
2. `('visible', 'will be preserved')`

## AGENT-005: provider outdated

Fixture: `provider-outdated`.

1. `('command', 'Agent Integrations')`
2. `('visible', 'Update')`

## COORD-001: queued task

Fixture: `queued-task`.

1. `('command', 'Settings')`
2. `('click', 'Tasks and Agents')`
3. `('visible', 'Queued for delivery')`

## COORD-002: waiting agent

Fixture: `waiting-agent`.

1. `('command', 'Settings')`
2. `('click', 'Tasks and Agents')`
3. `('visible', 'Waiting for agent readiness')`

## COORD-003: accepted task

Fixture: `accepted-task`.

1. `('command', 'Settings')`
2. `('click', 'Tasks and Agents')`
3. `('visible', 'awaiting acknowledgement')`

## COORD-004: acknowledged task

Fixture: `acknowledged-task`.

1. `('command', 'Settings')`
2. `('click', 'Tasks and Agents')`
3. `('visible', 'Acknowledged by agent')`

## COORD-005: failed task

Fixture: `failed-task`.

1. `('command', 'Settings')`
2. `('click', 'Tasks and Agents')`
3. `('visible', 'Delivery failed')`

## COORD-006: completed task

Fixture: `completed-task`.

1. `('command', 'Settings')`
2. `('click', 'Tasks and Agents')`
3. `('visible', 'fixture-result')`

## REMOTE-001: remote connected

Fixture: `remote-connected`.

1. `('remote_shell',)`
2. `('remote_workspace_preserved',)`

## REMOTE-002: remote offline

Fixture: `remote-offline`.

1. `('visible', 'Offline')`

## REMOTE-003: remote auth rejected

Fixture: `remote-auth-rejected`.

1. `('visible', 'Offline')`
2. `('absent', 'SSH Authentication')`
3. `('remote_rejected', 'Permission denied')`
4. `('context', 'Fixture', 'machine')`
5. `('enabled', 'Reconnect', 'menu')`
6. `('key', 'ESCAPE')`
7. `('remote_workspace_preserved',)`

## REMOTE-004: remote persistence disabled

Fixture: `remote-persistence-disabled`.

1. `('visible', 'Session persistence warning for Fixture', 60)`

## REMOTE-005: remote incompatible

Fixture: `remote-incompatible`.

1. `('visible', 'Takeover required')`

## UPDATE-001: update available

Fixture: `update-available`.

1. `('visible', 'Upgrade…')`

## UPDATE-002: update required

Fixture: `update-required`.

1. `('visible', 'Upgrade…')`
2. `('click', 'Upgrade…')`
3. `('visible', 'Upgrade Jam')`
4. `('visible', 'Jam 0.0.5')`
5. `('visible', 'Fixture')`
6. `('visible', '1 running terminal')`
7. `('click', 'Cancel', 'button')`
8. `('remote_workspace_preserved',)`

## UPDATE-003: update interrupted

Fixture: `update-interrupted`.

1. `('visible', 'Recover…')`

## REMOTE-010: Cancel real SSH continue

Fixture: `remote-password-required`.

1. `('context', 'Fixture', 'machine')`
2. `('click', 'Reconnect', 'menu')`
3. `('dialog', 'SSH Authentication')`
4. `('dialog_button', 'Cancel')`
5. `('finish_dialogs',)`
6. `('absent', 'SSH Authentication')`
7. `('visible', 'Offline')`
8. `('remote_rejected', 'Permission denied')`
9. `('remote_workspace_preserved',)`

## REMOTE-011: Accept real SSH continue

Fixture: `remote-password-required`.

1. `('context', 'Fixture', 'machine')`
2. `('click', 'Reconnect', 'menu')`
3. `('dialog', 'SSH Authentication')`
4. `('type', 'fixture-local-only')`
5. `('dialog_button', 'Continue')`
6. `('dialog', 'SSH Authentication')`
7. `('type', 'fixture-local-only')`
8. `('dialog_button', 'Continue')`
9. `('dialog', 'SSH Authentication')`
10. `('type', 'fixture-local-only')`
11. `('dialog_button', 'Continue')`
12. `('finish_dialogs',)`
13. `('remote_shell',)`
14. `('remote_workspace_preserved',)`

## REMOTE-012: Cancel real SSH trust host

Fixture: `remote-host-untrusted`.

1. `('context', 'Fixture', 'machine')`
2. `('click', 'Reconnect', 'menu')`
3. `('dialog', 'SSH Authentication')`
4. `('dialog_button', 'Cancel')`
5. `('finish_dialogs',)`
6. `('absent', 'SSH Authentication')`
7. `('visible', 'Offline')`
8. `('remote_rejected', 'Host key verification failed')`
9. `('remote_workspace_preserved',)`

## REMOTE-013: Accept real SSH trust host

Fixture: `remote-host-untrusted`.

1. `('context', 'Fixture', 'machine')`
2. `('click', 'Reconnect', 'menu')`
3. `('dialog', 'SSH Authentication')`
4. `('dialog_button', 'Trust Host')`
5. `('finish_dialogs',)`
6. `('remote_shell',)`
7. `('remote_workspace_preserved',)`

## DEFAULT-001: Fresh profile uses documented worktrees_directory

Fixture: `isolated local installation`.

1. `('settings', 'General')`
2. `('save',)`
3. `('config', 'worktrees_directory', '~/wt/{{repo}}/{{worktree}}')`

## DEFAULT-002: Fresh profile uses documented copy_on_select

Fixture: `isolated local installation`.

1. `('settings', 'General')`
2. `('save',)`
3. `('config', 'copy_on_select', False)`

## DEFAULT-003: Fresh profile uses documented automatic_check_updates

Fixture: `isolated local installation`.

1. `('settings', 'General')`
2. `('save',)`
3. `('config', 'automatic_check_updates', True)`

## DEFAULT-004: Fresh profile uses documented enabled_agents

Fixture: `isolated local installation`.

1. `('settings', 'General')`
2. `('save',)`
3. `('config', 'enabled_agents', ['claude', 'codex'])`

## DEFAULT-005: Fresh profile uses documented agent_models

Fixture: `isolated local installation`.

1. `('settings', 'General')`
2. `('save',)`
3. `('config', 'agent_models', {})`

## DEFAULT-006: Fresh profile uses documented max_agents_per_coordinator

Fixture: `isolated local installation`.

1. `('settings', 'General')`
2. `('save',)`
3. `('config', 'max_agents_per_coordinator', 8)`

## DEFAULT-007: Fresh profile uses documented resume_interrupted_work

Fixture: `isolated local installation`.

1. `('settings', 'General')`
2. `('save',)`
3. `('config', 'resume_interrupted_work', True)`

## DEFAULT-008: Fresh profile uses documented appearance

Fixture: `isolated local installation`.

1. `('settings', 'General')`
2. `('save',)`
3. `('config', 'appearance', 'system')`

## DEFAULT-009: Fresh profile uses documented close_empty_workspaces

Fixture: `isolated local installation`.

1. `('settings', 'General')`
2. `('save',)`
3. `('config', 'close_empty_workspaces', False)`

## DEFAULT-010: Fresh profile uses documented close_empty_tabs

Fixture: `isolated local installation`.

1. `('settings', 'General')`
2. `('save',)`
3. `('config', 'close_empty_tabs', True)`

## DEFAULT-011: Fresh profile uses documented allow_workspace_deletion

Fixture: `isolated local installation`.

1. `('settings', 'General')`
2. `('save',)`
3. `('config', 'allow_workspace_deletion', True)`

## BIND-001: assign shortcut for Find in Tab History in settings

Fixture: `isolated local installation`.

1. `('settings', 'Keyboard')`
2. `('fill', 'Find in Tab History', 'ctrl+alt+j')`
3. `('field_value', 'Find in Tab History', 'ctrl+alt+j')`
4. `('save',)`
5. `('config', 'bindings.find', ['ctrl+alt+j'])`
6. `('relaunch',)`
7. `('settings', 'Keyboard')`
8. `('field_value', 'Find in Tab History', 'ctrl+alt+j')`

## BIND-002: unbind shortcut for Find in Tab History in settings

Fixture: `isolated local installation`.

1. `('settings', 'Keyboard')`
2. `('fill', 'Find in Tab History', '')`
3. `('field_value', 'Find in Tab History', '')`
4. `('save',)`
5. `('config', 'bindings.find', [])`
6. `('relaunch',)`
7. `('settings', 'Keyboard')`
8. `('field_value', 'Find in Tab History', '')`

## BIND-003: assign shortcut for Find Next in settings

Fixture: `isolated local installation`.

1. `('settings', 'Keyboard')`
2. `('fill', 'Find Next', 'ctrl+alt+j')`
3. `('field_value', 'Find Next', 'ctrl+alt+j')`
4. `('save',)`
5. `('config', 'bindings.find_next', ['ctrl+alt+j'])`
6. `('relaunch',)`
7. `('settings', 'Keyboard')`
8. `('field_value', 'Find Next', 'ctrl+alt+j')`

## BIND-004: unbind shortcut for Find Next in settings

Fixture: `isolated local installation`.

1. `('settings', 'Keyboard')`
2. `('fill', 'Find Next', '')`
3. `('field_value', 'Find Next', '')`
4. `('save',)`
5. `('config', 'bindings.find_next', [])`
6. `('relaunch',)`
7. `('settings', 'Keyboard')`
8. `('field_value', 'Find Next', '')`

## BIND-005: assign shortcut for Find Previous in settings

Fixture: `isolated local installation`.

1. `('settings', 'Keyboard')`
2. `('fill', 'Find Previous', 'ctrl+alt+j')`
3. `('field_value', 'Find Previous', 'ctrl+alt+j')`
4. `('save',)`
5. `('config', 'bindings.find_previous', ['ctrl+alt+j'])`
6. `('relaunch',)`
7. `('settings', 'Keyboard')`
8. `('field_value', 'Find Previous', 'ctrl+alt+j')`

## BIND-006: unbind shortcut for Find Previous in settings

Fixture: `isolated local installation`.

1. `('settings', 'Keyboard')`
2. `('fill', 'Find Previous', '')`
3. `('field_value', 'Find Previous', '')`
4. `('save',)`
5. `('config', 'bindings.find_previous', [])`
6. `('relaunch',)`
7. `('settings', 'Keyboard')`
8. `('field_value', 'Find Previous', '')`

## BIND-007: assign shortcut for Command Palette in settings

Fixture: `isolated local installation`.

1. `('settings', 'Keyboard')`
2. `('fill', 'Command Palette', 'ctrl+alt+j')`
3. `('field_value', 'Command Palette', 'ctrl+alt+j')`
4. `('save',)`
5. `('config', 'bindings.palette', ['ctrl+alt+j'])`
6. `('relaunch',)`
7. `('settings', 'Keyboard')`
8. `('field_value', 'Command Palette', 'ctrl+alt+j')`

## BIND-008: unbind shortcut for Command Palette in settings

Fixture: `isolated local installation`.

1. `('settings', 'Keyboard')`
2. `('fill', 'Command Palette', '')`
3. `('field_value', 'Command Palette', '')`
4. `('save',)`
5. `('config', 'bindings.palette', [])`
6. `('relaunch',)`
7. `('settings', 'Keyboard')`
8. `('field_value', 'Command Palette', '')`

## BIND-009: assign shortcut for New Workspace in settings

Fixture: `isolated local installation`.

1. `('settings', 'Keyboard')`
2. `('fill', 'New Workspace', 'ctrl+alt+j')`
3. `('field_value', 'New Workspace', 'ctrl+alt+j')`
4. `('save',)`
5. `('config', 'bindings.new_workspace', ['ctrl+alt+j'])`
6. `('relaunch',)`
7. `('settings', 'Keyboard')`
8. `('field_value', 'New Workspace', 'ctrl+alt+j')`

## BIND-010: unbind shortcut for New Workspace in settings

Fixture: `isolated local installation`.

1. `('settings', 'Keyboard')`
2. `('fill', 'New Workspace', '')`
3. `('field_value', 'New Workspace', '')`
4. `('save',)`
5. `('config', 'bindings.new_workspace', [])`
6. `('relaunch',)`
7. `('settings', 'Keyboard')`
8. `('field_value', 'New Workspace', '')`

## BIND-011: assign shortcut for Open Workspace in settings

Fixture: `isolated local installation`.

1. `('settings', 'Keyboard')`
2. `('fill', 'Open Workspace', 'ctrl+alt+j')`
3. `('field_value', 'Open Workspace', 'ctrl+alt+j')`
4. `('save',)`
5. `('config', 'bindings.open_workspace', ['ctrl+alt+j'])`
6. `('relaunch',)`
7. `('settings', 'Keyboard')`
8. `('field_value', 'Open Workspace', 'ctrl+alt+j')`

## BIND-012: unbind shortcut for Open Workspace in settings

Fixture: `isolated local installation`.

1. `('settings', 'Keyboard')`
2. `('fill', 'Open Workspace', '')`
3. `('field_value', 'Open Workspace', '')`
4. `('save',)`
5. `('config', 'bindings.open_workspace', [])`
6. `('relaunch',)`
7. `('settings', 'Keyboard')`
8. `('field_value', 'Open Workspace', '')`

## BIND-013: assign shortcut for New Tab in settings

Fixture: `isolated local installation`.

1. `('settings', 'Keyboard')`
2. `('fill', 'New Tab', 'ctrl+alt+j')`
3. `('field_value', 'New Tab', 'ctrl+alt+j')`
4. `('save',)`
5. `('config', 'bindings.new_tab', ['ctrl+alt+j'])`
6. `('relaunch',)`
7. `('settings', 'Keyboard')`
8. `('field_value', 'New Tab', 'ctrl+alt+j')`

## BIND-014: unbind shortcut for New Tab in settings

Fixture: `isolated local installation`.

1. `('settings', 'Keyboard')`
2. `('fill', 'New Tab', '')`
3. `('field_value', 'New Tab', '')`
4. `('save',)`
5. `('config', 'bindings.new_tab', [])`
6. `('relaunch',)`
7. `('settings', 'Keyboard')`
8. `('field_value', 'New Tab', '')`

## BIND-015: assign shortcut for Split Right in settings

Fixture: `isolated local installation`.

1. `('settings', 'Keyboard')`
2. `('fill', 'Split Right', 'ctrl+alt+j')`
3. `('field_value', 'Split Right', 'ctrl+alt+j')`
4. `('save',)`
5. `('config', 'bindings.split_right', ['ctrl+alt+j'])`
6. `('relaunch',)`
7. `('settings', 'Keyboard')`
8. `('field_value', 'Split Right', 'ctrl+alt+j')`

## BIND-016: unbind shortcut for Split Right in settings

Fixture: `isolated local installation`.

1. `('settings', 'Keyboard')`
2. `('fill', 'Split Right', '')`
3. `('field_value', 'Split Right', '')`
4. `('save',)`
5. `('config', 'bindings.split_right', [])`
6. `('relaunch',)`
7. `('settings', 'Keyboard')`
8. `('field_value', 'Split Right', '')`

## BIND-017: assign shortcut for Split Down in settings

Fixture: `isolated local installation`.

1. `('settings', 'Keyboard')`
2. `('fill', 'Split Down', 'ctrl+alt+j')`
3. `('field_value', 'Split Down', 'ctrl+alt+j')`
4. `('save',)`
5. `('config', 'bindings.split_down', ['ctrl+alt+j'])`
6. `('relaunch',)`
7. `('settings', 'Keyboard')`
8. `('field_value', 'Split Down', 'ctrl+alt+j')`

## BIND-018: unbind shortcut for Split Down in settings

Fixture: `isolated local installation`.

1. `('settings', 'Keyboard')`
2. `('fill', 'Split Down', '')`
3. `('field_value', 'Split Down', '')`
4. `('save',)`
5. `('config', 'bindings.split_down', [])`
6. `('relaunch',)`
7. `('settings', 'Keyboard')`
8. `('field_value', 'Split Down', '')`

## BIND-019: assign shortcut for New Worktree in settings

Fixture: `isolated local installation`.

1. `('settings', 'Keyboard')`
2. `('fill', 'New Worktree', 'ctrl+alt+j')`
3. `('field_value', 'New Worktree', 'ctrl+alt+j')`
4. `('save',)`
5. `('config', 'bindings.new_worktree', ['ctrl+alt+j'])`
6. `('relaunch',)`
7. `('settings', 'Keyboard')`
8. `('field_value', 'New Worktree', 'ctrl+alt+j')`

## BIND-020: unbind shortcut for New Worktree in settings

Fixture: `isolated local installation`.

1. `('settings', 'Keyboard')`
2. `('fill', 'New Worktree', '')`
3. `('field_value', 'New Worktree', '')`
4. `('save',)`
5. `('config', 'bindings.new_worktree', [])`
6. `('relaunch',)`
7. `('settings', 'Keyboard')`
8. `('field_value', 'New Worktree', '')`

## BIND-021: assign shortcut for Open Worktree in settings

Fixture: `isolated local installation`.

1. `('settings', 'Keyboard')`
2. `('fill', 'Open Worktree', 'ctrl+alt+j')`
3. `('field_value', 'Open Worktree', 'ctrl+alt+j')`
4. `('save',)`
5. `('config', 'bindings.open_worktree', ['ctrl+alt+j'])`
6. `('relaunch',)`
7. `('settings', 'Keyboard')`
8. `('field_value', 'Open Worktree', 'ctrl+alt+j')`

## BIND-022: unbind shortcut for Open Worktree in settings

Fixture: `isolated local installation`.

1. `('settings', 'Keyboard')`
2. `('fill', 'Open Worktree', '')`
3. `('field_value', 'Open Worktree', '')`
4. `('save',)`
5. `('config', 'bindings.open_worktree', [])`
6. `('relaunch',)`
7. `('settings', 'Keyboard')`
8. `('field_value', 'Open Worktree', '')`

## BIND-023: assign shortcut for Close Pane in settings

Fixture: `isolated local installation`.

1. `('settings', 'Keyboard')`
2. `('fill', 'Close Pane', 'ctrl+alt+j')`
3. `('field_value', 'Close Pane', 'ctrl+alt+j')`
4. `('save',)`
5. `('config', 'bindings.close_pane', ['ctrl+alt+j'])`
6. `('relaunch',)`
7. `('settings', 'Keyboard')`
8. `('field_value', 'Close Pane', 'ctrl+alt+j')`

## BIND-024: unbind shortcut for Close Pane in settings

Fixture: `isolated local installation`.

1. `('settings', 'Keyboard')`
2. `('fill', 'Close Pane', '')`
3. `('field_value', 'Close Pane', '')`
4. `('save',)`
5. `('config', 'bindings.close_pane', [])`
6. `('relaunch',)`
7. `('settings', 'Keyboard')`
8. `('field_value', 'Close Pane', '')`

## BIND-025: assign shortcut for Close Workspace in settings

Fixture: `isolated local installation`.

1. `('settings', 'Keyboard')`
2. `('fill', 'Close Workspace', 'ctrl+alt+j')`
3. `('field_value', 'Close Workspace', 'ctrl+alt+j')`
4. `('save',)`
5. `('config', 'bindings.close_workspace', ['ctrl+alt+j'])`
6. `('relaunch',)`
7. `('settings', 'Keyboard')`
8. `('field_value', 'Close Workspace', 'ctrl+alt+j')`

## BIND-026: unbind shortcut for Close Workspace in settings

Fixture: `isolated local installation`.

1. `('settings', 'Keyboard')`
2. `('fill', 'Close Workspace', '')`
3. `('field_value', 'Close Workspace', '')`
4. `('save',)`
5. `('config', 'bindings.close_workspace', [])`
6. `('relaunch',)`
7. `('settings', 'Keyboard')`
8. `('field_value', 'Close Workspace', '')`

## BIND-027: assign shortcut for Settings in settings

Fixture: `isolated local installation`.

1. `('settings', 'Keyboard')`
2. `('fill', 'Settings', 'ctrl+alt+j')`
3. `('field_value', 'Settings', 'ctrl+alt+j')`
4. `('save',)`
5. `('config', 'bindings.settings', ['ctrl+alt+j'])`
6. `('relaunch',)`
7. `('settings', 'Keyboard')`
8. `('field_value', 'Settings', 'ctrl+alt+j')`

## BIND-028: unbind shortcut for Settings in settings

Fixture: `isolated local installation`.

1. `('settings', 'Keyboard')`
2. `('fill', 'Settings', '')`
3. `('field_value', 'Settings', '')`
4. `('save',)`
5. `('config', 'bindings.settings', [])`
6. `('relaunch',)`
7. `('settings', 'Keyboard')`
8. `('field_value', 'Settings', '')`

## BIND-029: assign shortcut for Agent Integrations in settings

Fixture: `isolated local installation`.

1. `('settings', 'Keyboard')`
2. `('fill', 'Agent Integrations', 'ctrl+alt+j')`
3. `('field_value', 'Agent Integrations', 'ctrl+alt+j')`
4. `('save',)`
5. `('config', 'bindings.agent_integrations', ['ctrl+alt+j'])`
6. `('relaunch',)`
7. `('settings', 'Keyboard')`
8. `('field_value', 'Agent Integrations', 'ctrl+alt+j')`

## BIND-030: unbind shortcut for Agent Integrations in settings

Fixture: `isolated local installation`.

1. `('settings', 'Keyboard')`
2. `('fill', 'Agent Integrations', '')`
3. `('field_value', 'Agent Integrations', '')`
4. `('save',)`
5. `('config', 'bindings.agent_integrations', [])`
6. `('relaunch',)`
7. `('settings', 'Keyboard')`
8. `('field_value', 'Agent Integrations', '')`

## MODEL-001: Manual claude Coordinator model is persisted

Fixture: `isolated local installation`.

1. `('settings', 'Coordination')`
2. `('click', 'Claude')`
3. `('fill', 'Coordinator model', 'fixture-model-id')`
4. `('save',)`
5. `('config', 'agent_models.claude.coordinator', 'fixture-model-id')`

## MODEL-002: Manual claude Worker model is persisted

Fixture: `isolated local installation`.

1. `('settings', 'Coordination')`
2. `('click', 'Claude')`
3. `('fill', 'Worker model', 'fixture-model-id')`
4. `('save',)`
5. `('config', 'agent_models.claude.worker', 'fixture-model-id')`

## MODEL-003: Manual claude Reviewer model is persisted

Fixture: `isolated local installation`.

1. `('settings', 'Coordination')`
2. `('click', 'Claude')`
3. `('fill', 'Reviewer model', 'fixture-model-id')`
4. `('save',)`
5. `('config', 'agent_models.claude.reviewer', 'fixture-model-id')`

## MODEL-004: Manual claude Complex work model is persisted

Fixture: `isolated local installation`.

1. `('settings', 'Coordination')`
2. `('click', 'Claude')`
3. `('fill', 'Complex work model', 'fixture-model-id')`
4. `('save',)`
5. `('config', 'agent_models.claude.complex_work', 'fixture-model-id')`

## MODEL-005: Manual codex Coordinator model is persisted

Fixture: `isolated local installation`.

1. `('settings', 'Coordination')`
2. `('click', 'Codex')`
3. `('fill', 'Coordinator model', 'fixture-model-id')`
4. `('save',)`
5. `('config', 'agent_models.codex.coordinator', 'fixture-model-id')`

## MODEL-006: Manual codex Worker model is persisted

Fixture: `isolated local installation`.

1. `('settings', 'Coordination')`
2. `('click', 'Codex')`
3. `('fill', 'Worker model', 'fixture-model-id')`
4. `('save',)`
5. `('config', 'agent_models.codex.worker', 'fixture-model-id')`

## MODEL-007: Manual codex Reviewer model is persisted

Fixture: `isolated local installation`.

1. `('settings', 'Coordination')`
2. `('click', 'Codex')`
3. `('fill', 'Reviewer model', 'fixture-model-id')`
4. `('save',)`
5. `('config', 'agent_models.codex.reviewer', 'fixture-model-id')`

## MODEL-008: Manual codex Complex work model is persisted

Fixture: `isolated local installation`.

1. `('settings', 'Coordination')`
2. `('click', 'Codex')`
3. `('fill', 'Complex work model', 'fixture-model-id')`
4. `('save',)`
5. `('config', 'agent_models.codex.complex_work', 'fixture-model-id')`

## INPUT-001: Physical input letter reaches a raw PTY

Fixture: `isolated local installation`.

1. `('probe',)`
2. `('probe_key', 'x')`
3. `('probe_hex', '78')`

## INPUT-002: Physical input tab reaches a raw PTY

Fixture: `isolated local installation`.

1. `('probe',)`
2. `('probe_key', 'TAB')`
3. `('probe_hex', '09')`

## INPUT-003: Physical input enter reaches a raw PTY

Fixture: `isolated local installation`.

1. `('probe',)`
2. `('probe_key', 'ENTER')`
3. `('probe_hex', '0d')`

## INPUT-004: Physical input escape reaches a raw PTY

Fixture: `isolated local installation`.

1. `('probe',)`
2. `('probe_key', 'ESCAPE')`
3. `('probe_hex', '1b')`

## INPUT-005: Physical input control-a reaches a raw PTY

Fixture: `isolated local installation`.

1. `('probe',)`
2. `('probe_key', 'ctrl+a')`
3. `('probe_hex', '01')`

## INPUT-006: Physical input control-e reaches a raw PTY

Fixture: `isolated local installation`.

1. `('probe',)`
2. `('probe_key', 'ctrl+e')`
3. `('probe_hex', '05')`

## INPUT-007: Physical input control-z reaches a raw PTY

Fixture: `isolated local installation`.

1. `('probe',)`
2. `('probe_key', 'ctrl+z')`
3. `('probe_hex', '1a')`

## INPUT-008: Physical input unicode reaches a raw PTY

Fixture: `isolated local installation`.

1. `('probe',)`
2. `('probe_key', '日本語')`
3. `('probe_hex', 'e697a5e69cace8aa9e')`

## IME-001: Native preedit stays out of the PTY until its exact Unicode commit

Fixture: `isolated local installation`.

1. `('probe',)`
2. `('begin_composition',)`
3. `('probe_no_input',)`
4. `('finish_composition', True)`
5. `('probe_hex', 'c3a9')`

## IME-002: Cancelling native preedit leaks no input and ordinary typing resumes

Fixture: `isolated local installation`.

1. `('probe',)`
2. `('begin_composition',)`
3. `('probe_no_input',)`
4. `('finish_composition', False)`
5. `('probe_no_input',)`
6. `('probe_key', 'x')`
7. `('probe_hex', '78')`

## IME-003: Real Japanese input keeps preedit out of the PTY and commits exact Hiragana

Fixture: `native-japanese-input`.

1. `('probe', 'raw', '60')`
2. `('input_source', 'Japanese')`
3. `('click', 'Terminal', 'terminal', 0)`
4. `('type', 'kana')`
5. `('key', 'F6')`
6. `('probe_no_input',)`
7. `('preedit_pixels',)`
8. `('key', 'ENTER')`
9. `('probe_hex', 'e3818be381aa')`
10. `('restore_input_source',)`

## IME-004: Cancelling real Japanese preedit leaks no PTY bytes and ordinary input resumes

Fixture: `native-japanese-input`.

1. `('probe', 'raw', '60')`
2. `('input_source', 'Japanese')`
3. `('click', 'Terminal', 'terminal', 0)`
4. `('type', 'kana')`
5. `('probe_no_input',)`
6. `('preedit_pixels',)`
7. `('key', 'ESCAPE')`
8. `('probe_no_input',)`
9. `('restore_input_source',)`
10. `('click', 'Terminal', 'terminal', 0)`
11. `('probe_key', 'x')`
12. `('probe_hex', '78')`

## GEOM-001: Native resize updates PTY geometry and preserves the shell

Fixture: `isolated local installation`.

1. `('resize', 1000, 660)`
2. `('remember_pids',)`
3. `('remember_layout',)`
4. `('geometry_probe',)`
5. `('resize', 900, 580)`
6. `('geometry_changed',)`
7. `('same_layout',)`
8. `('same_pids',)`

## GEOM-002: Native resize updates PTY geometry and preserves split panes

Fixture: `isolated local installation`.

1. `('resize', 1000, 660)`
2. `('command', 'Split Right')`
3. `('count', 'panes', 2)`
4. `('remember_pids',)`
5. `('remember_layout',)`
6. `('geometry_probe',)`
7. `('resize', 900, 580)`
8. `('geometry_changed',)`
9. `('same_layout',)`
10. `('same_pids',)`

## LONG-001: A long Unicode tab title stays accessible and survives client relaunch

Fixture: `isolated local installation`.

1. `('resize', 900, 580)`
2. `('remember_layout',)`
3. `('remember_pids',)`
4. `('clipboard', 'Review 日本語 long workspace notes long workspace notes long workspace notes long workspace notes long workspace notes long workspace notes long workspace notes long workspace notes')`
5. `('context', 'Tab 1')`
6. `('click', 'Rename')`
7. `('fill', 'Tab name', 'Review 日本語 long workspace notes long workspace notes long workspace notes long workspace notes long workspace notes long workspace notes long workspace notes long workspace notes')`
8. `('field_value', 'Tab name', 'Review 日本語 long workspace notes long workspace notes long workspace notes long workspace notes long workspace notes long workspace notes long workspace notes long workspace notes')`
9. `('key', 'ENTER')`
10. `('tab_name', 'Review 日本語 long workspace notes long workspace notes long workspace notes long workspace notes long workspace notes long workspace notes long workspace notes long workspace notes')`
11. `('target_visible', 'Review 日本語 long workspace notes long workspace notes long workspace notes long workspace notes long workspace notes long workspace notes long workspace notes long workspace notes', 'button')`
12. `('run_focused', 'printf long-title-focus')`
13. `('output', 'long-title-focus')`
14. `('relaunch',)`
15. `('target_visible', 'Review 日本語 long workspace notes long workspace notes long workspace notes long workspace notes long workspace notes long workspace notes long workspace notes long workspace notes', 'button')`
16. `('same_layout',)`
17. `('same_pids',)`

## DRAG-001: Drag Tab 1 over Tab 2 preserves processes

Fixture: `isolated local installation`.

1. `('command', 'New Tab')`
2. `('key', 'ESCAPE')`
3. `('count', 'tabs', 2)`
4. `('count', 'panes', 2)`
5. `('remember_tab_order',)`
6. `('remember_pids',)`
7. `('drag', 'Tab 1', 'Tab 2', 0.75)`
8. `('count', 'tabs', 2)`
9. `('tab_order_reversed',)`
10. `('same_pids',)`

## DRAG-002: Drag Tab 2 over Tab 1 preserves processes

Fixture: `isolated local installation`.

1. `('command', 'New Tab')`
2. `('key', 'ESCAPE')`
3. `('count', 'tabs', 2)`
4. `('count', 'panes', 2)`
5. `('remember_tab_order',)`
6. `('remember_pids',)`
7. `('drag', 'Tab 2', 'Tab 1', 0.25)`
8. `('count', 'tabs', 2)`
9. `('tab_order_reversed',)`
10. `('same_pids',)`

## INPUT-009: A physical key press and release reach the PTY keyboard protocol

Fixture: `isolated local installation`.

1. `('probe', 'keyboard')`
2. `('probe_key', 'x')`
3. `('probe_key_events',)`

## MOUSE-001: A physical click sends mouse press and release to the PTY

Fixture: `isolated local installation`.

1. `('probe', 'mouse')`
2. `('click', 'Terminal', 'terminal')`
3. `('probe_mouse_events',)`

## LIFECFG-001: close_empty_tabs=False controls the last pane lifecycle

Fixture: `isolated local installation`.

1. `('write_config', 'close_empty_tabs = false\n')`
2. `('relaunch',)`
3. `('command', 'Close Pane')`
4. `('count', 'panes', 0)`
5. `('count', 'tabs', 1)`
6. `('count', 'workspaces', 1)`

## LIFECFG-002: close_empty_tabs=True controls the last pane lifecycle

Fixture: `isolated local installation`.

1. `('write_config', 'close_empty_tabs = true\n')`
2. `('relaunch',)`
3. `('command', 'Close Pane')`
4. `('count', 'panes', 0)`
5. `('count', 'tabs', 0)`
6. `('count', 'workspaces', 1)`

## LIFECFG-003: close_empty_workspaces=True controls the last pane lifecycle

Fixture: `isolated local installation`.

1. `('write_config', 'close_empty_workspaces = true\n')`
2. `('relaunch',)`
3. `('command', 'Close Pane')`
4. `('count', 'panes', 0)`
5. `('count', 'tabs', 0)`
6. `('count', 'workspaces', 0)`

## PALETTE-001: Palette can be cancelled after searching nothing

Fixture: `isolated local installation`.

1. `('chord', 'ctrl+shift+p')`
2. `('fill', 'Search commands, workspaces, tabs, and panes', 'nothing')`
3. `('key', 'ESCAPE')`
4. `('run_focused', 'printf palette-returned-focus')`
5. `('output', 'palette-returned-focus')`

## PALETTE-002: Palette can be cancelled after searching Tab 1

Fixture: `isolated local installation`.

1. `('chord', 'ctrl+shift+p')`
2. `('fill', 'Search commands, workspaces, tabs, and panes', 'Tab 1')`
3. `('key', 'ESCAPE')`
4. `('run_focused', 'printf palette-returned-focus')`
5. `('output', 'palette-returned-focus')`

## PALETTE-003: Palette can be cancelled after searching Split Right

Fixture: `isolated local installation`.

1. `('chord', 'ctrl+shift+p')`
2. `('fill', 'Search commands, workspaces, tabs, and panes', 'Split Right')`
3. `('key', 'ESCAPE')`
4. `('run_focused', 'printf palette-returned-focus')`
5. `('output', 'palette-returned-focus')`

## PALETTE-004: Palette can be cancelled after searching Settings

Fixture: `isolated local installation`.

1. `('chord', 'ctrl+shift+p')`
2. `('fill', 'Search commands, workspaces, tabs, and panes', 'Settings')`
3. `('key', 'ESCAPE')`
4. `('run_focused', 'printf palette-returned-focus')`
5. `('output', 'palette-returned-focus')`

## PALETTE-005: Palette can be cancelled after searching Coordinator

Fixture: `isolated local installation`.

1. `('chord', 'ctrl+shift+p')`
2. `('fill', 'Search commands, workspaces, tabs, and panes', 'Coordinator')`
3. `('key', 'ESCAPE')`
4. `('run_focused', 'printf palette-returned-focus')`
5. `('output', 'palette-returned-focus')`

## RECOVERY-001: stopped coordinator external user workflow

Fixture: `stopped-coordinator`.

1. `('click', 'Coordinator', 'row')`
2. `('click', 'Recover Conversation…')`
3. `('visible', 'Recover Coordinator')`

## RECOVERY-002: codex conversations external user workflow

Fixture: `codex-conversations`.

1. `('click', 'Coordinator', 'row')`
2. `('click', 'Recover Conversation…')`
3. `('visible', 'Jam coordinator')`

## RECOVERY-003: codex conversations external user workflow

Fixture: `codex-conversations`.

1. `('click', 'Coordinator', 'row')`
2. `('click', 'Recover Conversation…')`
3. `('click', 'Cancel')`
4. `('visible', 'Coordinator stopped')`

## RECOVERY-004: stopped coordinator external user workflow

Fixture: `stopped-coordinator`.

1. `('click', 'Coordinator', 'row')`
2. `('click', 'Resume')`
3. `('visible', 'fixture-agent-resumed')`

## RECOVERY-005: provider launch failed external user workflow

Fixture: `provider-launch-failed`.

1. `('click', 'Coordinator', 'row')`
2. `('visible', 'Resume')`
3. `('visible', 'fixture-launch-error')`

## USAGE-001: usage two providers external user workflow

Fixture: `usage-two-providers`.

1. `('visible', 'Codex')`
2. `('visible', 'Claude')`
3. `('visible', '5h')`

## USAGE-002: usage weekly external user workflow

Fixture: `usage-weekly`.

1. `('visible', '7d')`

## USAGE-003: usage unknown account external user workflow

Fixture: `usage-unknown-account`.

1. `('click', 'Codex', 'button')`
2. `('visible', 'Codex usage')`
3. `('visible', 'Account identity unavailable. Shown separately for this machine.')`
4. `('visible', 'Local')`

## USAGE-004: usage no sessions external user workflow

Fixture: `usage-no-sessions`.

1. `('absent', 'USAGE')`

## USAGE-005: usage auth error external user workflow

Fixture: `usage-auth-error`.

1. `('visible', 'Sign in again to read usage')`
2. `('click', 'Codex, fixture@example.invalid', 'button')`
3. `('visible', 'Codex usage')`
4. `('visible', 'Sign in again to read usage')`
5. `('key', 'ESCAPE')`
6. `('run', 'printf usage-auth-shell-alive')`
7. `('output', 'usage-auth-shell-alive')`

## USAGE-006: usage rate limited external user workflow

Fixture: `usage-rate-limited`.

1. `('visible', 'Usage service rate limited. Retrying later')`
2. `('click', 'Codex, fixture@example.invalid', 'button')`
3. `('visible', 'Codex usage')`
4. `('visible', 'Usage service rate limited. Retrying later')`
5. `('key', 'ESCAPE')`
6. `('run', 'printf usage-rate-shell-alive')`
7. `('output', 'usage-rate-shell-alive')`

## REMOTE-006: remote connected external user workflow

Fixture: `remote-connected`.

1. `('remote_shell',)`
2. `('context', 'Fixture', 'machine')`
3. `('click', 'Disconnect', 'menu')`
4. `('context', 'Fixture', 'machine')`
5. `('disabled', 'Disconnect')`
6. `('enabled', 'Reconnect', 'menu')`
7. `('key', 'ESCAPE')`
8. `('remote_workspace_preserved',)`

## REMOTE-007: remote connected external user workflow

Fixture: `remote-connected`.

1. `('remote_shell',)`
2. `('context', 'Fixture', 'machine')`
3. `('click', 'Edit Remote…', 'menu')`
4. `('visible', 'Save')`
5. `('click', 'Cancel')`

## REMOTE-008: remote connected external user workflow

Fixture: `remote-connected`.

1. `('context', 'Fixture', 'machine')`
2. `('click', 'Remove Remote', 'menu')`
3. `('machine_absent', 'Fixture')`
4. `('remote_workspace_preserved',)`

## REMOTE-009: remote persistence disabled external user workflow

Fixture: `remote-persistence-disabled`.

1. `('click', 'Session persistence warning for Fixture')`
2. `('visible', 'Enable Persistence')`

## SERVER-001: remote update available external user workflow

Fixture: `remote-update-available`.

1. `('remote_shell',)`
2. `('context', 'Fixture', 'machine')`
3. `('click', 'Update Server…', 'menu')`
4. `('visible', 'Update When Sessions End')`

## SERVER-002: remote update available external user workflow

Fixture: `remote-update-available`.

1. `('remote_shell',)`
2. `('context', 'Fixture', 'machine')`
3. `('click', 'Update Server…', 'menu')`
4. `('click', 'Update When Sessions End', 'button')`
5. `('context', 'Fixture', 'machine')`
6. `('visible', 'Cancel Scheduled Update')`

## SERVER-003: remote update available external user workflow

Fixture: `remote-update-available`.

1. `('remote_shell',)`
2. `('context', 'Fixture', 'machine')`
3. `('click', 'Update Server…', 'menu')`
4. `('click', 'Update When Sessions End', 'button')`
5. `('context', 'Fixture', 'machine')`
6. `('click', 'Cancel Scheduled Update', 'menu')`
7. `('context', 'Fixture', 'machine')`
8. `('absent', 'Cancel Scheduled Update')`

## SERVER-004: remote connected external user workflow

Fixture: `remote-connected`.

1. `('remote_shell',)`
2. `('context', 'Fixture', 'machine')`
3. `('click', 'Restart Server…', 'menu')`
4. `('visible', 'Restart Server — Fixture')`
5. `('visible', '1 running terminal')`
6. `('click', 'Cancel', 'button')`

## UPDATE-004: update available external user workflow

Fixture: `update-available`.

1. `('click', 'Skip This Version')`
2. `('absent', 'Upgrade…')`
3. `('relaunch',)`
4. `('absent', 'Upgrade…')`

## UPDATE-005: update available external user workflow

Fixture: `update-available`.

1. `('click', 'Upgrade…')`
2. `('visible', 'Upgrade and Restart')`
3. `('click', 'Cancel')`

## UPDATE-006: update interrupted external user workflow

Fixture: `update-interrupted`.

1. `('click', 'Recover…')`
2. `('visible', 'Recover')`
3. `('click', 'Cancel')`

## UPDATE-007: update bad signature external user workflow

Fixture: `update-bad-signature`.

1. `('settings', 'General')`
2. `('click', 'Check for Updates…', 'button')`
3. `('click', 'Details', 'disclosure')`
4. `('visible', 'signature')`

## UPDATE-008: update bad checksum external user workflow

Fixture: `update-bad-checksum`.

1. `('click', 'Upgrade…')`
2. `('click', 'Details', 'disclosure')`
3. `('visible', 'checksum')`

## UPDATE-009: update download failed external user workflow

Fixture: `update-download-failed`.

1. `('remember_pids',)`
2. `('remember_layout',)`
3. `('click', 'Upgrade…')`
4. `('click', 'Details', 'disclosure')`
5. `('visible', 'http status: 503')`
6. `('click', 'Cancel')`
7. `('installed_version', '0.0.4')`
8. `('same_layout',)`
9. `('same_pids',)`

## UPDATE-010: update available external user workflow

Fixture: `update-available`.

1. `('run', 'printf x >> update-executions')`
2. `('remember_layout',)`
3. `('remember_client',)`
4. `('click', 'Upgrade…')`
5. `('enabled', 'Upgrade and Restart', 'button')`
6. `('click', 'Upgrade and Restart', 'button')`
7. `('installed_version', '0.0.5')`
8. `('client_restarted',)`
9. `('visible', 'Open Release Notes')`
10. `('same_layout',)`
11. `('file_text', 'update-executions', 'x')`

## UPDATE-011: update interrupted external user workflow

Fixture: `update-interrupted`.

1. `('remember_layout',)`
2. `('remember_pids',)`
3. `('click', 'Recover…')`
4. `('click', 'Recover', 'button')`
5. `('absent', 'Recover Interrupted Upgrade')`
6. `('installed_version', '0.0.4')`
7. `('same_layout',)`
8. `('same_pids',)`

## UPDATE-012: update rollback external user workflow

Fixture: `update-rollback`.

1. `('run', 'printf x >> rollback-executions')`
2. `('remember_layout',)`
3. `('click', 'Upgrade…')`
4. `('enabled', 'Upgrade and Restart', 'button')`
5. `('click', 'Upgrade and Restart', 'button')`
6. `('upgrade_rolled_back',)`
7. `('installed_version', '0.0.4')`
8. `('relaunch',)`
9. `('same_layout',)`
10. `('file_text', 'rollback-executions', 'x')`
11. `('run', 'printf y >> rollback-executions')`
12. `('file_text', 'rollback-executions', 'xy')`

## WT-006: dirty checkout external user workflow

Fixture: `dirty-checkout`.

1. `('context', 'Fixture checkout')`
2. `('click', 'Delete Checkout…')`
3. `('visible', 'Delete')`
4. `('click', 'Cancel')`
5. `('file_exists', '../Fixture checkout/dirty.txt')`

## WT-007: checkout deletion disabled external user workflow

Fixture: `checkout-deletion-disabled`.

1. `('context', 'Fixture checkout')`
2. `('absent', 'Delete Checkout…')`

## WT-008: existing checkouts external user workflow

Fixture: `existing-checkouts`.

1. `('command', 'Open Worktree')`
2. `('fill', 'Filter by branch or path', 'feature/existing')`
3. `('visible', 'feature/existing')`

## WT-009: existing checkouts external user workflow

Fixture: `existing-checkouts`.

1. `('command', 'Open Worktree')`
2. `('fill', 'Filter by branch or path', 'feature/existing')`
3. `('enabled', 'Open', 'button')`
4. `('key', 'ENTER')`
5. `('count', 'workspaces', 2)`

## GIT-001: git diff external user workflow

Fixture: `git-diff`.

1. `('visible', '+3')`
2. `('visible', '−2')`

## GIT-002: github review external user workflow

Fixture: `github-review`.

1. `('visible', '#42')`

## GIT-003: gitlab review external user workflow

Fixture: `gitlab-review`.

1. `('visible', '!42')`

## AGENT-006: provider detected external user workflow

Fixture: `provider-detected`.

1. `('command', 'Agent Integrations')`
2. `('click', 'integration.claude.install')`
3. `('visible', 'Installed')`

## AGENT-007: provider installed external user workflow

Fixture: `provider-installed`.

1. `('command', 'Agent Integrations')`
2. `('click', 'integration.claude.remove')`
3. `('target_visible', 'integration.claude.install')`

## AGENT-008: provider foreign external user workflow

Fixture: `provider-foreign`.

1. `('command', 'Agent Integrations')`
2. `('visible', 'will be preserved')`
3. `('disabled', 'integration.claude.install')`
4. `('home_file_text', '.claude/skills/jam/SKILL.md', 'User-owned foreign skill. Preserve these bytes.\n')`

## REMOTE-014: Enabling persistence changes the remote OS setting without restarting its sessions

Fixture: `remote-persistence-disabled`.

1. `('remote_linger', 'no')`
2. `('click', 'Session persistence warning for Fixture', 'button')`
3. `('enabled', 'Enable Persistence', 'button')`
4. `('click', 'Enable Persistence', 'button')`
5. `('remote_linger', 'yes')`
6. `('absent', 'Session persistence warning for Fixture')`
7. `('remote_workspace_preserved',)`
8. `('remote_shell',)`

## MULTI-001: Two independent clients share live terminals; closing one preserves the other

Fixture: `isolated local installation`.

1. `('remember_layout',)`
2. `('remember_pids',)`
3. `('open_client', 'second')`
4. `('count', 'workspaces', 1)`
5. `('count', 'tabs', 1)`
6. `('count', 'panes', 1)`
7. `('same_layout',)`
8. `('same_pids',)`
9. `('run', 'printf second-client-alive; printf x >> shared-client-executions')`
10. `('use_client', 'primary')`
11. `('visible', 'second-client-alive')`
12. `('close_client', 'second')`
13. `('run', 'printf primary-client-alive; printf y >> shared-client-executions')`
14. `('output', 'primary-client-alive')`
15. `('file_text', 'shared-client-executions', 'xy')`
16. `('same_layout',)`
17. `('same_pids',)`

## MULTI-002: Saving settings in another client invalidates the first client's open draft

Fixture: `isolated local installation`.

1. `('remember_pids',)`
2. `('settings', 'General')`
3. `('open_client', 'second')`
4. `('settings', 'General')`
5. `('click', 'Copy text on selection', 'toggle')`
6. `('save',)`
7. `('config', 'copy_on_select', True)`
8. `('use_client', 'primary')`
9. `('visible', 'Settings changed outside this window')`
10. `('disabled', 'Save')`
11. `('click', 'Cancel', 'button')`
12. `('settings', 'General')`
13. `('save',)`
14. `('config', 'copy_on_select', True)`
15. `('close_client', 'second')`
16. `('same_pids',)`

## MULTI-003: Tabs created and renamed in separate clients converge without replacing their shells

Fixture: `isolated local installation`.

1. `('open_client', 'second')`
2. `('command', 'New Tab')`
3. `('count', 'tabs', 2)`
4. `('remember_layout',)`
5. `('remember_pids',)`
6. `('use_client', 'primary')`
7. `('target_visible', 'Tab 2', 'button')`
8. `('context', 'Tab 2', 'button')`
9. `('click', 'Rename', 'menu')`
10. `('fill', 'Tab name', 'Shared client tab')`
11. `('key', 'ENTER')`
12. `('tab_name', 'Shared client tab')`
13. `('use_client', 'second')`
14. `('target_visible', 'Shared client tab', 'button')`
15. `('same_layout',)`
16. `('same_pids',)`
17. `('close_client', 'second')`
18. `('same_layout',)`
19. `('same_pids',)`

## MULTI-004: The focused client owns shared PTY geometry without inactive window resize churn

Fixture: `isolated local installation`.

1. `('resize', 1000, 660)`
2. `('remember_layout',)`
3. `('remember_pids',)`
4. `('geometry_probe', 120)`
5. `('geometry_record', 'primary')`
6. `('open_client', 'second')`
7. `('resize', 900, 580)`
8. `('geometry_differs', 'primary')`
9. `('geometry_record', 'second')`
10. `('use_client', 'primary')`
11. `('geometry_matches', 'primary')`
12. `('use_client', 'second')`
13. `('geometry_matches', 'second')`
14. `('close_client', 'second')`
15. `('geometry_matches', 'primary')`
16. `('geometry_stop',)`
17. `('same_layout',)`
18. `('same_pids',)`
19. `('run', 'printf geometry-owner-restored')`
20. `('output', 'geometry-owner-restored')`

## SERVER-005: A forced server update restores layout without replaying shell commands

Fixture: `remote-update-available`.

1. `('remote_run', 'printf x >> /home/fixture/project/server-executions')`
2. `('context', 'Fixture', 'machine')`
3. `('enabled', 'Update Server…', 'menu')`
4. `('click', 'Update Server…', 'menu')`
5. `('enabled', 'Update and Restart', 'button')`
6. `('click', 'Update and Restart', 'button')`
7. `('remote_server_changed', 'update')`
8. `('remote_file_text', 'server-executions', 'x')`
9. `('remote_run', 'printf y >> /home/fixture/project/server-executions')`
10. `('remote_file_text', 'server-executions', 'xy')`

## SERVER-006: A scheduled update runs after the last remote shell exits

Fixture: `remote-update-available`.

1. `('remote_run', 'printf x >> /home/fixture/project/server-executions')`
2. `('context', 'Fixture', 'machine')`
3. `('enabled', 'Update Server…', 'menu')`
4. `('click', 'Update Server…', 'menu')`
5. `('enabled', 'Update When Sessions End', 'button')`
6. `('click', 'Update When Sessions End', 'button')`
7. `('remote_exit',)`
8. `('remote_server_changed', 'update')`
9. `('remote_file_text', 'server-executions', 'x')`
10. `('remote_run', 'printf y >> /home/fixture/project/server-executions')`
11. `('remote_file_text', 'server-executions', 'xy')`

## SERVER-007: Restarting a remote server preserves layout and starts fresh working shells

Fixture: `remote-connected`.

1. `('remote_run', 'printf x >> /home/fixture/project/restart-executions')`
2. `('context', 'Fixture', 'machine')`
3. `('enabled', 'Restart Server…', 'menu')`
4. `('click', 'Restart Server…', 'menu')`
5. `('enabled', 'Restart Server', 'button')`
6. `('click', 'Restart Server', 'button')`
7. `('remote_server_changed', 'restart')`
8. `('remote_file_text', 'restart-executions', 'x')`
9. `('remote_run', 'printf y >> /home/fixture/project/restart-executions')`
10. `('remote_file_text', 'restart-executions', 'xy')`

## SERVER-008: Cancelling a scheduled server update prevents replacement after sessions end

Fixture: `remote-update-available`.

1. `('remote_run', 'printf x >> /home/fixture/project/cancelled-update-executions')`
2. `('context', 'Fixture', 'machine')`
3. `('enabled', 'Update Server…', 'menu')`
4. `('click', 'Update Server…', 'menu')`
5. `('enabled', 'Update When Sessions End', 'button')`
6. `('click', 'Update When Sessions End', 'button')`
7. `('context', 'Fixture', 'machine')`
8. `('click', 'Cancel Scheduled Update', 'menu')`
9. `('remote_exit',)`
10. `('remote_server_stable', 10)`
11. `('remote_workspace_preserved',)`
12. `('remote_file_text', 'cancelled-update-executions', 'x')`

## VISUAL-001: Actual light terminal pixels remain legible after reopening the client

Fixture: `isolated local installation`.

1. `('resize', 900, 580)`
2. `('remember_pids',)`
3. `('remember_layout',)`
4. `('settings', 'Appearance')`
5. `('click', 'Light')`
6. `('save',)`
7. `('config', 'appearance', 'light')`
8. `('run', "printf '\\033[2J\\033[H\\116\\101\\124\\111\\126\\105\\040\\124\\105\\122\\115\\111\\116\\101\\114\\040\\067\\064\\062\\071\\n'")`
9. `('visible', 'NATIVE TERMINAL 7429')`
10. `('visual_text', 'NATIVE TERMINAL 7429', 'light')`
11. `('relaunch',)`
12. `('visual_text', 'NATIVE TERMINAL 7429', 'light')`
13. `('same_layout',)`
14. `('same_pids',)`

## VISUAL-002: Actual dark terminal pixels remain legible after reopening the client

Fixture: `isolated local installation`.

1. `('resize', 900, 580)`
2. `('remember_pids',)`
3. `('remember_layout',)`
4. `('settings', 'Appearance')`
5. `('click', 'Dark')`
6. `('save',)`
7. `('config', 'appearance', 'dark')`
8. `('run', "printf '\\033[2J\\033[H\\116\\101\\124\\111\\126\\105\\040\\124\\105\\122\\115\\111\\116\\101\\114\\040\\067\\064\\062\\071\\n'")`
9. `('visible', 'NATIVE TERMINAL 7429')`
10. `('visual_text', 'NATIVE TERMINAL 7429', 'dark')`
11. `('relaunch',)`
12. `('visual_text', 'NATIVE TERMINAL 7429', 'dark')`
13. `('same_layout',)`
14. `('same_pids',)`

## VISUAL-003: Resizing preserves actual rendered terminal text and its live shell

Fixture: `isolated local installation`.

1. `('resize', 1000, 660)`
2. `('remember_layout',)`
3. `('remember_pids',)`
4. `('run', "printf '\\033[2J\\033[H\\122\\105\\116\\104\\105\\122\\040\\101\\106\\124\\105\\122\\040\\122\\105\\123\\111\\132\\105\\040\\065\\070\\063\\067\\n'")`
5. `('visual_text', 'RENDER AFTER RESIZE 5837')`
6. `('resize', 900, 580)`
7. `('visual_text', 'RENDER AFTER RESIZE 5837')`
8. `('same_layout',)`
9. `('same_pids',)`

## UPDATE-013: A new terminal in another client invalidates the prepared upgrade confirmation

Fixture: `update-available`.

1. `('click', 'Upgrade…')`
2. `('enabled', 'Upgrade and Restart', 'button')`
3. `('open_client', 'second')`
4. `('command', 'New Tab')`
5. `('count', 'tabs', 2)`
6. `('remember_layout',)`
7. `('remember_pids',)`
8. `('use_client', 'primary')`
9. `('click', 'Upgrade and Restart', 'button')`
10. `('visible', 'Sessions changed after confirmation')`
11. `('installed_version', '0.0.4')`
12. `('same_layout',)`
13. `('same_pids',)`
14. `('close_client', 'second')`
15. `('same_layout',)`
16. `('same_pids',)`

## TERM-010: Opening the requested directory does not show a spurious launch error

Fixture: `isolated local installation`.

1. `('count', 'workspaces', 1)`
2. `('run', 'pwd; printf launch-directory-ready')`
3. `('output', 'launch-directory-ready')`
4. `('output_repo',)`
5. `('absent', 'Could not open the launch directory')`

## SELECT-001: copy_on_select=False controls physical drag selection and explicit Copy

Fixture: `isolated local installation`.

1. `('resize', 900, 580)`
2. `('remember_layout',)`
3. `('remember_pids',)`
4. `('settings', 'General')`
5. `('save',)`
6. `('config', 'copy_on_select', False)`
7. `('run', "printf '\\033[2J\\033[H\\123\\105\\114\\105\\103\\124\\111\\117\\116\\n'")`
8. `('clipboard', 'selection clipboard sentinel')`
9. `('select_word', 'SELECTION')`
10. `('clipboard_stable', 'selection clipboard sentinel')`
11. `('copy',)`
12. `('clipboard_equals', 'SELECTION')`
13. `('same_layout',)`
14. `('same_pids',)`

## SELECT-002: copy_on_select=True controls physical drag selection and explicit Copy

Fixture: `isolated local installation`.

1. `('resize', 900, 580)`
2. `('remember_layout',)`
3. `('remember_pids',)`
4. `('settings', 'General')`
5. `('click', 'Copy text on selection', 'toggle')`
6. `('save',)`
7. `('config', 'copy_on_select', True)`
8. `('run', "printf '\\033[2J\\033[H\\123\\105\\114\\105\\103\\124\\111\\117\\116\\n'")`
9. `('clipboard', 'selection clipboard sentinel')`
10. `('select_word', 'SELECTION')`
11. `('clipboard_equals', 'SELECTION')`
12. `('copy',)`
13. `('clipboard_equals', 'SELECTION')`
14. `('same_layout',)`
15. `('same_pids',)`

## WT-010: Cancelling checkout closure preserves files, layout and running terminals

Fixture: `dirty-checkout`.

1. `('remember_layout',)`
2. `('remember_pids',)`
3. `('context', 'Fixture checkout')`
4. `('click', 'Close Workspace', 'menu')`
5. `('visible', 'Close workspace?')`
6. `('click', 'Cancel', 'button')`
7. `('same_layout',)`
8. `('same_pids',)`
9. `('file_text', '../Fixture checkout/dirty.txt', 'Keep this uncommitted work\n')`
10. `('git_branch', 'feature/existing')`

## WT-011: Closing a checkout preserves its dirty files and branch

Fixture: `dirty-checkout`.

1. `('remember_pids',)`
2. `('context', 'Fixture checkout')`
3. `('click', 'Close Workspace', 'menu')`
4. `('visible', 'Close workspace?')`
5. `('click', 'Close', 'button')`
6. `('count', 'workspaces', 1)`
7. `('count', 'panes', 1)`
8. `('remaining_pids',)`
9. `('file_text', '../Fixture checkout/dirty.txt', 'Keep this uncommitted work\n')`
10. `('git_branch', 'feature/existing')`
11. `('run', 'pwd')`
12. `('output_repo',)`

## WT-014: Disabling checkout deletion removes its action across client relaunch

Fixture: `dirty-checkout`.

1. `('remember_layout',)`
2. `('remember_pids',)`
3. `('settings', 'General')`
4. `('click', 'Allow deleting checkout files', 'toggle')`
5. `('save',)`
6. `('config', 'allow_workspace_deletion', False)`
7. `('context', 'Fixture checkout')`
8. `('target_visible', 'Close Workspace', 'menu')`
9. `('absent', 'Delete Checkout…')`
10. `('key', 'ESCAPE')`
11. `('relaunch',)`
12. `('config', 'allow_workspace_deletion', False)`
13. `('context', 'Fixture checkout')`
14. `('target_visible', 'Close Workspace', 'menu')`
15. `('absent', 'Delete Checkout…')`
16. `('key', 'ESCAPE')`
17. `('same_layout',)`
18. `('same_pids',)`
19. `('file_text', '../Fixture checkout/dirty.txt', 'Keep this uncommitted work\n')`
20. `('git_branch', 'feature/existing')`

## WT-015: Re-enabling checkout deletion restores its action without deleting on cancellation

Fixture: `dirty-checkout`.

1. `('remember_layout',)`
2. `('remember_pids',)`
3. `('settings', 'General')`
4. `('click', 'Allow deleting checkout files', 'toggle')`
5. `('save',)`
6. `('config', 'allow_workspace_deletion', False)`
7. `('context', 'Fixture checkout')`
8. `('target_visible', 'Close Workspace', 'menu')`
9. `('absent', 'Delete Checkout…')`
10. `('key', 'ESCAPE')`
11. `('relaunch',)`
12. `('config', 'allow_workspace_deletion', False)`
13. `('context', 'Fixture checkout')`
14. `('target_visible', 'Close Workspace', 'menu')`
15. `('absent', 'Delete Checkout…')`
16. `('key', 'ESCAPE')`
17. `('settings', 'General')`
18. `('click', 'Allow deleting checkout files', 'toggle')`
19. `('save',)`
20. `('config', 'allow_workspace_deletion', True)`
21. `('context', 'Fixture checkout')`
22. `('click', 'Delete Checkout…', 'menu')`
23. `('visible', 'Delete checkout?')`
24. `('click', 'Cancel', 'button')`
25. `('same_layout',)`
26. `('same_pids',)`
27. `('file_text', '../Fixture checkout/dirty.txt', 'Keep this uncommitted work\n')`
28. `('git_branch', 'feature/existing')`

## WT-012: Cancelling force deletion preserves dirty files, layout and running terminals

Fixture: `dirty-checkout`.

1. `('remember_layout',)`
2. `('remember_pids',)`
3. `('context', 'Fixture checkout')`
4. `('click', 'Delete Checkout…', 'menu')`
5. `('visible', 'Delete checkout?')`
6. `('click', 'Delete', 'button')`
7. `('visible', 'Force delete checkout?')`
8. `('visible', 'Uncommitted and untracked files will be permanently lost.')`
9. `('click', 'Cancel', 'button')`
10. `('same_layout',)`
11. `('same_pids',)`
12. `('file_text', '../Fixture checkout/dirty.txt', 'Keep this uncommitted work\n')`
13. `('git_branch', 'feature/existing')`

## WT-013: Force deletion removes only the checkout and preserves its branch

Fixture: `dirty-checkout`.

1. `('remember_layout',)`
2. `('remember_pids',)`
3. `('context', 'Fixture checkout')`
4. `('click', 'Delete Checkout…', 'menu')`
5. `('visible', 'Delete checkout?')`
6. `('click', 'Delete', 'button')`
7. `('visible', 'Force delete checkout?')`
8. `('visible', 'Uncommitted and untracked files will be permanently lost.')`
9. `('click', 'Force Delete', 'button')`
10. `('count', 'workspaces', 1)`
11. `('count', 'panes', 1)`
12. `('remaining_pids',)`
13. `('path_absent', '../Fixture checkout')`
14. `('file_text', 'tracked.txt', 'first\nsecond\n')`
15. `('run', 'pwd')`
16. `('output_repo',)`
17. `('git_branch', 'feature/existing')`

## COORD-007: Start a Claude coordinator through its workspace menu

Fixture: `provider-detected`.

1. `('context', 'Fixture')`
2. `('click', 'Add Coordinator')`
3. `('click', 'Claude')`
4. `('visible', 'fixture-agent-resumed')`
5. `('agent_field', 'provider', 'claude')`

## COORD-008: Start a Codex coordinator through its workspace menu

Fixture: `provider-detected`.

1. `('context', 'Fixture')`
2. `('click', 'Add Coordinator')`
3. `('click', 'Codex')`
4. `('visible', 'fixture-agent-resumed')`
5. `('agent_field', 'provider', 'codex')`

## COORD-009: Pause a coordinator through Tasks and Agents

Fixture: `single-coordinator`.

1. `('settings', 'Tasks and Agents')`
2. `('click', 'Pause')`
3. `('visible', 'Resume')`
4. `('agent_field', 'paused', True)`

## COORD-010: Resume retains the conversation without submitting input

Fixture: `single-coordinator`.

1. `('remember_agent_session',)`
2. `('remember_provider_inputs',)`
3. `('settings', 'Tasks and Agents')`
4. `('click', 'Pause')`
5. `('visible', 'Resume')`
6. `('click', 'Resume')`
7. `('agent_field', 'paused', False)`
8. `('agent_field', 'ready', True)`
9. `('same_agent_session',)`
10. `('provider_inputs_unchanged',)`

## COORD-011: Instructions include the workspace's actual instruction file

Fixture: `single-coordinator`.

1. `('settings', 'Tasks and Agents')`
2. `('click', 'Instructions')`
3. `('visible', 'Fixture instruction: preserve user work.')`

## COORD-012: Finished reviewed work is hidden until archived history is requested

Fixture: `archived-task`.

1. `('settings', 'Tasks and Agents')`
2. `('absent', 'Fixture task objective')`
3. `('click', 'Include archived')`
4. `('visible', 'Fixture task objective')`
5. `('visible', 'Archived · done')`

## AGENT-009: Installing and removing claude integration preserves the user's hooks and settings

Fixture: `provider-custom-hooks`.

1. `('command', 'Agent Integrations')`
2. `('click', 'integration.claude.install')`
3. `('target_visible', 'integration.claude.remove')`
4. `('home_json_contains', '.claude/settings.json', {'fixture_user_setting': {'keep': True}, 'hooks': {'SessionStart': [{'matcher': '', 'hooks': [{'type': 'command', 'command': 'printf fixture-user-hook'}]}]}})`
5. `('click', 'integration.claude.remove')`
6. `('target_visible', 'integration.claude.install')`
7. `('home_json_equals', '.claude/settings.json', {'fixture_user_setting': {'keep': True}, 'hooks': {'SessionStart': [{'matcher': '', 'hooks': [{'type': 'command', 'command': 'printf fixture-user-hook'}]}]}})`

## AGENT-010: Installing and removing codex integration preserves the user's hooks and settings

Fixture: `provider-custom-hooks`.

1. `('command', 'Agent Integrations')`
2. `('click', 'integration.codex.install')`
3. `('target_visible', 'integration.codex.remove')`
4. `('home_json_contains', '.codex/hooks.json', {'fixture_user_setting': {'keep': True}, 'hooks': {'SessionStart': [{'matcher': '', 'hooks': [{'type': 'command', 'command': 'printf fixture-user-hook'}]}]}})`
5. `('click', 'integration.codex.remove')`
6. `('target_visible', 'integration.codex.install')`
7. `('home_json_equals', '.codex/hooks.json', {'fixture_user_setting': {'keep': True}, 'hooks': {'SessionStart': [{'matcher': '', 'hooks': [{'type': 'command', 'command': 'printf fixture-user-hook'}]}]}})`

## IDENTITY-001: Reported Codex identity survives activity acknowledgement and clear

Fixture: `isolated local installation`.

Behaviors: `identity`.

1. `('program_identity', 'codex')`
2. `('pane_field', 'agent_provider', 'codex')`
3. `('pane_field', 'agent_source', 'reported')`
4. `('pane_field', 'agent_role', None)`
5. `('visible', 'Codex')`
6. `('program_phase', 'done')`
7. `('pane_field', 'status', 'done')`
8. `('click', 'Terminal', 'terminal')`
9. `('pane_field', 'status', 'unknown')`
10. `('pane_field', 'agent_provider', 'codex')`
11. `('visible', 'Codex')`
12. `('program_phase', 'clear')`
13. `('pane_field', 'agent_provider', 'codex')`
14. `('program_phase', 'exit')`
15. `('pane_field', 'agent_provider', None)`
16. `('pane_field', 'agent_source', None)`
17. `('run', 'printf identity-returned-to-shell')`
18. `('output', 'identity-returned-to-shell')`

## IDENTITY-002: Reported Claude identity survives activity acknowledgement and clear

Fixture: `isolated local installation`.

Behaviors: `identity`.

1. `('program_identity', 'claude-code')`
2. `('pane_field', 'agent_provider', 'claude')`
3. `('pane_field', 'agent_source', 'reported')`
4. `('pane_field', 'agent_role', None)`
5. `('visible', 'Claude')`
6. `('program_phase', 'done')`
7. `('pane_field', 'status', 'done')`
8. `('click', 'Terminal', 'terminal')`
9. `('pane_field', 'status', 'unknown')`
10. `('pane_field', 'agent_provider', 'claude')`
11. `('visible', 'Claude')`
12. `('program_phase', 'clear')`
13. `('pane_field', 'agent_provider', 'claude')`
14. `('program_phase', 'exit')`
15. `('pane_field', 'agent_provider', None)`
16. `('pane_field', 'agent_source', None)`
17. `('run', 'printf identity-returned-to-shell')`
18. `('output', 'identity-returned-to-shell')`

## IDENTITY-003: A terminal title mentioning a provider does not register an agent

Fixture: `isolated local installation`.

Behaviors: `identity`.

1. `('run', "printf '\\033]0;codex build report\\007'")`
2. `('pane_field', 'agent_provider', None)`
3. `('pane_field', 'agent_source', None)`
4. `('pane_field', 'agent_role', None)`
5. `('run', 'printf title-is-not-an-agent')`
6. `('output', 'title-is-not-an-agent')`

## IDENTITY-004: A client relaunch retains reported identity while a server restart clears it

Fixture: `isolated local installation`.

Behaviors: `identity`.

1. `('remember_layout',)`
2. `('remember_pids',)`
3. `('program_identity', 'codex')`
4. `('pane_field', 'agent_provider', 'codex')`
5. `('pane_field', 'agent_source', 'reported')`
6. `('relaunch',)`
7. `('same_pids',)`
8. `('pane_field', 'agent_provider', 'codex')`
9. `('pane_field', 'agent_source', 'reported')`
10. `('restart_server',)`
11. `('same_layout',)`
12. `('pane_field', 'agent_provider', None)`
13. `('pane_field', 'agent_source', None)`
14. `('run', 'printf restarted-plain-shell')`
15. `('output', 'restarted-plain-shell')`

## IDENTITY-005: A registered coordinator shows its provider and role in the terminal header

Fixture: `single-coordinator`.

Behaviors: `identity`.

1. `('click', 'Coordinator', 'row')`
2. `('visible', 'fixture-agent-resumed')`
3. `('visible', 'Codex · Coordinator')`
4. `('agent_field', 'provider', 'codex')`
5. `('registered_pane_fields', {'agent_provider': 'codex', 'agent_source': 'registered', 'agent_role': 'coordinator'})`

## IMAGE-001: PNG paste writes a private image on the local host

Fixture: `isolated local installation`.

Behaviors: `images`.

1. `('run', 'true')`
2. `('clipboard_image', 'png', True, False)`
3. `('type_terminal', "printf '%s\\n' ")`
4. `('paste',)`
5. `('type_terminal', 'queued-after-image > image-path.txt')`
6. `('key', 'ENTER')`
7. `('image_paste_result', 'png', False)`

## IMAGE-002: TIFF paste writes a private image on the local host

Fixture: `isolated local installation`.

Behaviors: `images`.

1. `('run', 'true')`
2. `('clipboard_image', 'tiff', True, False)`
3. `('type_terminal', "printf '%s\\n' ")`
4. `('context', 'Terminal', 'terminal')`
5. `('click', 'Paste', 'menu')`
6. `('type_terminal', 'queued-after-image > image-path.txt')`
7. `('key', 'ENTER')`
8. `('image_paste_result', 'tiff', False)`

## IMAGE-003: PNG paste writes a private image on the remote host

Fixture: `remote-connected`.

Behaviors: `images`.

1. `('remote_shell',)`
2. `('clipboard_image', 'png', True, False)`
3. `('type_terminal', "printf '%s\\n' ")`
4. `('paste',)`
5. `('type_terminal', 'queued-after-image > image-path.txt')`
6. `('key', 'ENTER')`
7. `('image_paste_result', 'png', True)`

## IMAGE-004: Control-V in a reported agent pane pastes the image before queued input

Fixture: `isolated local installation`.

Behaviors: `images`, `identity`.

1. `('program_identity', 'codex', 'image-arguments')`
2. `('pane_field', 'agent_source', 'reported')`
3. `('clipboard_image', 'png', True, False)`
4. `('chord', 'ctrl+v')`
5. `('type_terminal', 'queued-after-image')`
6. `('key', 'ENTER')`
7. `('image_paste_result', 'png', False)`

## IMAGE-005: Oversized clipboard image is rejected without typing into the terminal

Fixture: `isolated local installation`.

Behaviors: `images`.

1. `('probe',)`
2. `('clipboard_image', 'png', True, True)`
3. `('paste',)`
4. `('visible', 'Clipboard image is too large')`
5. `('probe_no_input',)`
6. `('probe_key', 'x')`
7. `('probe_hex', '78')`

## IMAGE-006: A server without image capability leaves its terminal usable

Fixture: `remote-limited`.

Behaviors: `images`.

1. `('remote_shell',)`
2. `('clipboard_image', 'png', True, False)`
3. `('paste',)`
4. `('visible', 'Image pasting needs an updated Jam server on this machine')`
5. `('remote_shell', 'printf image-error-shell-usable')`
6. `('remote_workspace_preserved',)`

## CREATE-001: Failed worktree creation can be retried once after its base ref is repaired

Fixture: `isolated local installation`.

Behaviors: `creation`.

1. `('command', 'New Worktree')`
2. `('fill', 'Branch name', 'creation-retry')`
3. `('fill', 'Base revision', 'repairable-base')`
4. `('click', 'Create', 'button')`
5. `('enabled', 'Retry', 'button')`
6. `('enabled', 'Dismiss', 'button')`
7. `('settle_count', 'workspaces', 1)`
8. `('git_no_branch', 'creation-retry')`
9. `('git_update_ref', 'repairable-base')`
10. `('click', 'Retry', 'button')`
11. `('created_workspace', 'wt/Project ü with spaces/creation-retry', False)`
12. `('run', 'printf new-worktree-ready > creation-result.txt')`
13. `('home_file_text', 'wt/Project ü with spaces/creation-retry/creation-result.txt', 'new-worktree-ready')`
14. `('settle_count', 'workspaces', 2)`
15. `('git_branch', 'creation-retry')`

## CREATE-002: Dismissing a failed worktree restores the previous usable terminal

Fixture: `isolated local installation`.

Behaviors: `creation`.

1. `('remember_layout',)`
2. `('remember_pids',)`
3. `('command', 'New Worktree')`
4. `('fill', 'Branch name', 'creation-dismiss')`
5. `('fill', 'Base revision', 'invalid-creation-base')`
6. `('click', 'Create', 'button')`
7. `('enabled', 'Retry', 'button')`
8. `('click', 'Dismiss', 'button')`
9. `('absent', 'Retry')`
10. `('same_layout',)`
11. `('same_pids',)`
12. `('git_no_branch', 'creation-dismiss')`
13. `('run', 'printf creation-dismissed')`
14. `('output', 'creation-dismissed')`

## CREATE-003: A capability-limited remote creates one workspace in the remote home

Fixture: `remote-limited`.

Behaviors: `creation`, `creation-recovery`.

1. `('remote_shell',)`
2. `('command', 'New Workspace')`
3. `('created_workspace', '/home/fixture', True)`
4. `('type_terminal', 'printf remote-creation-ready > /home/fixture/project/creation-result.txt')`
5. `('key', 'ENTER')`
6. `('remote_file_text', 'creation-result.txt', 'remote-creation-ready')`

## TAKEOVER-001: Cancelling a legacy server takeover preserves its running sessions

Fixture: `remote-legacy`.

Behaviors: `takeover`.

1. `('visible', 'Takeover required', 60)`
2. `('context', 'Fixture', 'machine')`
3. `('click', 'Take Over Server…', 'menu')`
4. `('visible', 'Take over the server on Fixture?')`
5. `('visible', 'Running terminals will stop')`
6. `('enabled', 'Take Over and Reconnect', 'button')`
7. `('click', 'Cancel', 'button')`
8. `('remote_workspace_preserved',)`
9. `('remote_server_stable', 2)`

## TAKEOVER-002: An explicit legacy takeover restores the saved layout and starts fresh shells

Fixture: `remote-legacy`.

Behaviors: `takeover`.

1. `('visible', 'Takeover required', 60)`
2. `('context', 'Fixture', 'machine')`
3. `('click', 'Take Over Server…', 'menu')`
4. `('enabled', 'Take Over and Reconnect', 'button')`
5. `('click', 'Take Over and Reconnect', 'button')`
6. `('visible', 'Remote project', 120)`
7. `('click', 'Remote project', 'row')`
8. `('remote_server_changed', 'takeover')`
9. `('remote_file_text', 'takeover-executions', 'x')`
10. `('remote_shell', 'printf y >> takeover-executions')`
11. `('remote_file_text', 'takeover-executions', 'xy')`

## DIAG-001: Inspecting an unresponsive local server preserves its daemon and live shell

Fixture: `isolated local installation`.

Behaviors: `diagnostics`.

1. `('remember_layout',)`
2. `('remember_pids',)`
3. `('suspend_daemon',)`
4. `('visible', 'Local server is not responding', 45)`
5. `('context', 'Local', 'machine')`
6. `('click', 'Server Details…', 'menu')`
7. `('visible', 'Server — Local')`
8. `('visible', 'without stopping running sessions')`
9. `('enabled', 'Retry', 'button')`
10. `('click', 'Retry', 'button')`
11. `('enabled', 'Retry', 'button')`
12. `('visible', 'without stopping running sessions')`
13. `('resume_daemon',)`
14. `('absent', 'Server — Local')`
15. `('same_layout',)`
16. `('same_pids',)`
17. `('run', 'printf daemon-recovered-without-restart')`
18. `('output', 'daemon-recovered-without-restart')`

## DIAG-002: A broken SSH path exposes inspection and Retry without enabling destructive actions

Fixture: `remote-creation-transport`.

Behaviors: `diagnostics`.

1. `('remote_shell',)`
2. `('peer_transport', True)`
3. `('visible', 'Server unavailable', 60)`
4. `('context', 'Fixture', 'machine')`
5. `('disabled', 'Update Server…', 'menu')`
6. `('disabled', 'Restart Server…', 'menu')`
7. `('absent', 'Take Over Server…')`
8. `('click', 'Server Details…', 'menu')`
9. `('visible', 'Server — Fixture')`
10. `('visible', 'without stopping running sessions')`
11. `('enabled', 'Retry', 'button')`
12. `('click', 'Retry', 'button')`
13. `('enabled', 'Retry', 'button')`
14. `('remote_workspace_preserved',)`
15. `('remote_server_stable', 2)`
16. `('peer_transport', False)`
17. `('absent', 'Server — Fixture')`
18. `('remote_shell',)`
19. `('remote_workspace_preserved',)`

## UPDATE-SETTINGS-001: Settings prepares and cancels one managed upgrade sheet

Fixture: `update-available`.

Behaviors: `update-settings`.

1. `('remember_layout',)`
2. `('remember_pids',)`
3. `('settings', 'General')`
4. `('click', 'Check for Updates…', 'button')`
5. `('visible', 'Jam 0.0.5 is available')`
6. `('enabled', 'Upgrade…', 'button')`
7. `('click', 'Upgrade…', 'button')`
8. `('visible', 'Upgrade Jam')`
9. `('enabled', 'Upgrade and Restart', 'button')`
10. `('click', 'Cancel', 'button')`
11. `('absent', 'Upgrade Jam')`
12. `('installed_version', '0.0.4')`
13. `('same_layout',)`
14. `('same_pids',)`

## ACCOUNT-001: A configured login appears without any active agent session

Fixture: `usage-configured-login`.

Behaviors: `accounts`.

1. `('visible', 'USAGE')`
2. `('click', 'Codex, fixture@example.invalid', 'button')`
3. `('visible', 'Codex usage')`
4. `('visible', 'fixture@example.invalid')`
5. `('visible', '0 active sessions')`
6. `('key', 'ESCAPE')`
7. `('pane_field', 'agent_provider', None)`
8. `('run', 'printf configured-account-shell-usable')`
9. `('output', 'configured-account-shell-usable')`

## RENDER-001: Synchronized output hides partial redraws

Fixture: `isolated local installation`.

Behaviors: `rendering`.

1. `('remember_pids',)`
2. `('synchronized_frames', 'complete')`
3. `('run', 'printf input-after-synchronized-output')`
4. `('output', 'input-after-synchronized-output')`
5. `('same_pids',)`

## RENDER-002: An unterminated synchronized redraw releases its frame after the deadline

Fixture: `isolated local installation`.

Behaviors: `rendering`.

1. `('remember_pids',)`
2. `('synchronized_frames', 'timeout')`
3. `('run', 'printf input-after-synchronized-output')`
4. `('output', 'input-after-synchronized-output')`
5. `('same_pids',)`

## RENDER-003: A focused terminal renders steady, blinking and concealed cursor states

Fixture: `isolated local installation`.

Behaviors: `rendering`.

1. `('remember_pids',)`
2. `('cursor_frames',)`
3. `('run', 'printf input-after-cursor-changes')`
4. `('output', 'input-after-cursor-changes')`
5. `('same_pids',)`

## CREATE-004: A pending worktree becomes one selected workspace with a usable terminal

Fixture: `isolated local installation`.

Behaviors: `creation`.

1. `('hold_worktree_checkout',)`
2. `('command', 'New Worktree')`
3. `('fill', 'Branch name', 'pending-creation')`
4. `('fill', 'Base revision', 'HEAD')`
5. `('click', 'Create', 'button')`
6. `('checkout_pending',)`
7. `('visible', 'Opening workspace')`
8. `('release_worktree_checkout',)`
9. `('created_workspace', 'wt/Project ü with spaces/pending-creation', False)`
10. `('absent', 'Opening workspace')`
11. `('run', 'printf pending-worktree-ready > creation-result.txt')`
12. `('home_file_text', 'wt/Project ü with spaces/pending-creation/creation-result.txt', 'pending-worktree-ready')`
13. `('settle_count', 'workspaces', 2)`

## CREATE-005: An interrupted creation resumes its existing operation after Retry

Fixture: `isolated local installation`.

Behaviors: `creation`, `creation-recovery`.

1. `('hold_worktree_checkout',)`
2. `('command', 'New Worktree')`
3. `('fill', 'Branch name', 'pending-creation')`
4. `('fill', 'Base revision', 'HEAD')`
5. `('click', 'Create', 'button')`
6. `('checkout_pending',)`
7. `('visible', 'Opening workspace')`
8. `('suspend_daemon',)`
9. `('visible', 'Retry', 35)`
10. `('resume_daemon',)`
11. `('release_worktree_checkout',)`
12. `('click', 'Retry', 'button')`
13. `('created_workspace', 'wt/Project ü with spaces/pending-creation', False)`
14. `('absent', 'Opening workspace')`
15. `('run', 'printf pending-worktree-ready > creation-result.txt')`
16. `('home_file_text', 'wt/Project ü with spaces/pending-creation/creation-result.txt', 'pending-worktree-ready')`
17. `('settle_count', 'workspaces', 2)`

## CLOSE-001: A workspace with no live terminals closes without a confirmation

Fixture: `isolated local installation`.

Behaviors: `focus-close`.

1. `('write_config', 'close_empty_tabs = false\nclose_empty_workspaces = false\n')`
2. `('relaunch',)`
3. `('type_terminal', 'exit')`
4. `('key', 'ENTER')`
5. `('visible', 'No terminals')`
6. `('count', 'workspaces', 1)`
7. `('command', 'Close Workspace')`
8. `('settle_count', 'workspaces', 0)`
9. `('file_text', 'tracked.txt', 'first\nsecond\n')`

## CLOSE-002: A live worktree requires confirmation before closing an empty project

Fixture: `dirty-checkout`.

Behaviors: `focus-close`.

1. `('write_config', 'close_empty_tabs = false\nclose_empty_workspaces = false\n')`
2. `('relaunch',)`
3. `('click', 'Fixture', 'row')`
4. `('type_terminal', 'exit')`
5. `('key', 'ENTER')`
6. `('visible', 'No terminals')`
7. `('remember_pids',)`
8. `('live_panes', 1)`
9. `('context', 'Fixture', 'row')`
10. `('click', 'Close Workspace', 'menu')`
11. `('visible', 'Close project and its open worktrees?')`
12. `('click', 'Cancel', 'button')`
13. `('same_pids',)`
14. `('context', 'Fixture', 'row')`
15. `('click', 'Close Workspace', 'menu')`
16. `('visible', 'Close project and its open worktrees?')`
17. `('click', 'Close', 'button')`
18. `('settle_count', 'workspaces', 0)`
19. `('settle_count', 'panes', 0)`
20. `('file_text', 'tracked.txt', 'first\nsecond\n')`
21. `('file_text', '../Fixture checkout/dirty.txt', 'Keep this uncommitted work\n')`
22. `('git_branch', 'feature/existing')`

## CLOSE-003: A live coordinator requires confirmation before closing its otherwise empty project

Fixture: `single-coordinator`.

Behaviors: `focus-close`.

1. `('write_config', 'close_empty_tabs = false\nclose_empty_workspaces = false\n')`
2. `('relaunch',)`
3. `('click', 'Fixture', 'row')`
4. `('type_terminal', 'exit')`
5. `('key', 'ENTER')`
6. `('visible', 'No terminals')`
7. `('remember_pids',)`
8. `('live_panes', 1)`
9. `('context', 'Fixture', 'row')`
10. `('click', 'Close Workspace', 'menu')`
11. `('visible', 'Close project and its open worktrees?')`
12. `('click', 'Cancel', 'button')`
13. `('same_pids',)`
14. `('context', 'Fixture', 'row')`
15. `('click', 'Close Workspace', 'menu')`
16. `('visible', 'Close project and its open worktrees?')`
17. `('click', 'Close', 'button')`
18. `('settle_count', 'workspaces', 0)`
19. `('settle_count', 'panes', 0)`
20. `('file_text', 'tracked.txt', 'first\nsecond\n')`

## IMAGE-007: Control-V is delivered as ordinary terminal input outside an agent pane

Fixture: `isolated local installation`.

Behaviors: `images`.

1. `('probe',)`
2. `('clipboard_image', 'png', True, False)`
3. `('chord', 'ctrl+v')`
4. `('probe_hex', '16')`

## IMAGE-008: A failed upload keeps prior files and leaves the terminal usable

Fixture: `isolated local installation`.

Behaviors: `images`.

1. `('block_clipboard_storage',)`
2. `('probe',)`
3. `('clipboard_image', 'png', True, False)`
4. `('paste',)`
5. `('visible', 'state/runtime directory must be an owned private directory')`
6. `('probe_no_input',)`
7. `('clipboard_storage_unchanged',)`
8. `('probe_key', 'x')`
9. `('probe_hex', '78')`

## CREATE-006: Opening a local directory twice selects its one existing workspace

Fixture: `isolated local installation`.

Behaviors: `creation`.

1. `('open_directory', 'Opened folder ü')`
2. `('created_workspace', 'Opened folder ü', False)`
3. `('run', 'printf preserved > open-result.txt')`
4. `('home_file_text', 'Opened folder ü/open-result.txt', 'preserved')`
5. `('remember_layout',)`
6. `('remember_pids',)`
7. `('open_directory', 'Opened folder ü')`
8. `('same_layout',)`
9. `('same_pids',)`
10. `('settle_count', 'workspaces', 2)`

## CREATE-007: Opening a remote directory twice keeps one remote workspace

Fixture: `remote-connected`.

Behaviors: `creation`.

1. `('remote_shell', 'mkdir -p /home/fixture/opened-directory')`
2. `('command', 'Open Workspace')`
3. `('visible', 'Open Remote Workspace')`
4. `('fill', 'Path', '/home/fixture/opened-directory')`
5. `('click', 'Go', 'button')`
6. `('enabled', 'Open', 'button')`
7. `('click', 'Open', 'button')`
8. `('created_workspace', '/home/fixture/opened-directory', True)`
9. `('command', 'Open Workspace')`
10. `('fill', 'Path', '/home/fixture/opened-directory')`
11. `('click', 'Go', 'button')`
12. `('enabled', 'Open', 'button')`
13. `('click', 'Open', 'button')`
14. `('created_workspace', '/home/fixture/opened-directory', True)`

## TAKEOVER-003: A newer server offers a client upgrade and preserves its live sessions

Fixture: `update-required`.

Behaviors: `takeover`, `update-settings`.

1. `('visible', 'Update Jam required', 60)`
2. `('context', 'Fixture', 'machine')`
3. `('enabled', 'Update Jam…', 'menu')`
4. `('absent', 'Take Over Server…')`
5. `('disabled', 'Update Server…', 'menu')`
6. `('key', 'ESCAPE')`
7. `('remote_workspace_preserved',)`
8. `('remote_server_stable', 2)`

## FOCUS-001: A background agent's completion is acknowledged only when its tab gains focus

Fixture: `isolated local installation`.

Behaviors: `focus-close`, `identity`.

1. `('remember_pane',)`
2. `('program_identity', 'codex', 'done-on-file')`
3. `('pane_field', 'agent_source', 'reported')`
4. `('command', 'New Tab')`
5. `('target_visible', 'Tab 2', 'button')`
6. `('release_program_completion',)`
7. `('remembered_pane_field', 'status', 'done')`
8. `('run', 'printf foreground-other-tab')`
9. `('output', 'foreground-other-tab')`
10. `('remembered_pane_field', 'status', 'done')`
11. `('click', 'Tab 1', 'button')`
12. `('remembered_pane_field', 'status', 'unknown')`
13. `('remembered_pane_field', 'agent_provider', 'codex')`
14. `('program_phase', 'exit')`
15. `('remembered_pane_field', 'agent_provider', None)`

## ACCOUNT-002: The same configured login on two machines shares one account page

Fixture: `usage-remote-account`.

Behaviors: `accounts`.

1. `('remote_shell',)`
2. `('usage_requests_from_both_hosts',)`
3. `('visible', 'fixture@example.invalid')`
4. `('visible', 'Claude')`
5. `('absent', 'Next usage account')`
6. `('click', 'Codex, fixture@example.invalid', 'button')`
7. `('visible', '0 active sessions')`
8. `('key', 'ESCAPE')`
9. `('context', 'Fixture', 'machine')`
10. `('click', 'Disconnect', 'menu')`
11. `('click', 'Codex, fixture@example.invalid', 'button')`
12. `('visible', '0 active sessions')`
13. `('absent', 'Machine unavailable. Showing last known usage.')`
14. `('key', 'ESCAPE')`
15. `('remote_workspace_preserved',)`

## ACCOUNT-003: Distinct accounts retain separate selectable pages across hosts

Fixture: `usage-remote-other-account`.

Behaviors: `accounts`.

1. `('remote_shell',)`
2. `('usage_requests_from_both_hosts',)`
3. `('visible', 'second@example.invalid')`
4. `('click', 'Next usage account', 'button')`
5. `('visible', 'fixture@example.invalid')`
6. `('visible', 'Claude')`
7. `('click', 'Previous usage account', 'button')`
8. `('visible', 'second@example.invalid')`
9. `('click', 'Codex, second@example.invalid', 'button')`
10. `('visible', '0 active sessions')`
11. `('key', 'ESCAPE')`
12. `('context', 'Fixture', 'machine')`
13. `('click', 'Disconnect', 'menu')`
14. `('click', 'Codex, second@example.invalid', 'button')`
15. `('visible', 'Machine unavailable. Showing last known usage.')`
16. `('visible', '25%')`
17. `('key', 'ESCAPE')`
18. `('context', 'Fixture', 'machine')`
19. `('click', 'Reconnect', 'menu')`
20. `('remote_shell',)`
21. `('click', 'Codex, second@example.invalid', 'button')`
22. `('absent', 'Machine unavailable. Showing last known usage.')`
23. `('key', 'ESCAPE')`
24. `('remote_workspace_preserved',)`

## ACCOUNT-004: One email shared by different organizations does not combine provider accounts

Fixture: `usage-remote-other-organization`.

Behaviors: `accounts`.

1. `('remote_shell',)`
2. `('usage_requests_from_both_hosts',)`
3. `('visible', '67%')`
4. `('usage_page_cycle', ('67%', '25%', '38%'))`
5. `('remote_workspace_preserved',)`

## ACCOUNT-005: Disabling a provider hides its usage and leaves the other provider available

Fixture: `usage-configured-login`.

Behaviors: `accounts`.

1. `('visible', 'Claude')`
2. `('visible', 'Codex')`
3. `('settings', 'Agents')`
4. `('click', 'Claude', 'toggle')`
5. `('save',)`
6. `('config', 'enabled_agents', ['codex'])`
7. `('absent', 'Claude')`
8. `('click', 'Codex, fixture@example.invalid', 'button')`
9. `('visible', 'Codex usage')`
10. `('key', 'ESCAPE')`
11. `('run', 'printf terminal-available-with-integration-disabled')`
12. `('output', 'terminal-available-with-integration-disabled')`

## ACCOUNT-006: The account page sums active provider processes across local and remote terminals

Fixture: `usage-remote-account`.

Behaviors: `accounts`.

1. `('run', "bash -c 'exec -a codex /bin/sleep 180' &")`
2. `('remote_shell', "bash -c 'exec -a codex /bin/sleep 180' &")`
3. `('usage_requests_from_both_hosts',)`
4. `('click', 'Codex, fixture@example.invalid', 'button')`
5. `('visible', '2 active sessions')`
6. `('key', 'ESCAPE')`
7. `('absent', 'Next usage account')`
8. `('remote_workspace_preserved',)`

## TAKEOVER-004: A compiled replacement that cannot start rolls back the remote server

Fixture: `remote-takeover-failure`.

Behaviors: `takeover`.

1. `('visible', 'Takeover required', 60)`
2. `('context', 'Fixture', 'machine')`
3. `('click', 'Take Over Server…', 'menu')`
4. `('enabled', 'Take Over and Reconnect', 'button')`
5. `('click', 'Take Over and Reconnect', 'button')`
6. `('visible', 'Try Again', 100)`
7. `('takeover_rolled_back',)`
8. `('remote_file_text', 'takeover-executions', 'x')`
9. `('enabled', 'Try Again', 'button')`
10. `('click', 'Cancel', 'button')`

## RENAME-001: The inline tab editor replaces the old rendered label and returns focus

Fixture: `isolated local installation`.

Behaviors: `rendering`, `focus-close`.

1. `('context', 'Tab 1', 'button')`
2. `('click', 'Rename', 'menu')`
3. `('fill', 'Tab name', 'BEFORE 9374')`
4. `('key', 'ENTER')`
5. `('tab_name', 'BEFORE 9374')`
6. `('rendered_label', 'BEFORE 9374', None)`
7. `('context', 'BEFORE 9374', 'button')`
8. `('click', 'Rename', 'menu')`
9. `('fill', 'Tab name', 'AFTER 2846')`
10. `('rendered_label', 'AFTER 2846', 'BEFORE 9374')`
11. `('key', 'ENTER')`
12. `('tab_name', 'AFTER 2846')`
13. `('run_focused', 'printf rename-returned-terminal-focus')`
14. `('output', 'rename-returned-terminal-focus')`

## CREATE-008: Completing a pending worktree keeps the workspace subsequently selected by the user

Fixture: `isolated local installation`.

Behaviors: `creation`.

1. `('hold_worktree_checkout',)`
2. `('command', 'New Worktree')`
3. `('fill', 'Branch name', 'background-creation')`
4. `('fill', 'Base revision', 'HEAD')`
5. `('click', 'Create', 'button')`
6. `('checkout_pending',)`
7. `('visible', 'Opening workspace')`
8. `('click', 'Project ü with spaces', 'row')`
9. `('release_worktree_checkout',)`
10. `('created_workspace', 'wt/Project ü with spaces/background-creation', False)`
11. `('run', 'printf original-selection-kept > selection-result.txt')`
12. `('file_text', 'selection-result.txt', 'original-selection-kept')`
13. `('settle_count', 'workspaces', 2)`

## CREATE-009: A remote checkout completed through a transport outage is found by explicit Retry

Fixture: `remote-creation-transport`.

Behaviors: `creation`, `creation-recovery`.

1. `('remote_shell',)`
2. `('hold_worktree_checkout', 'remote')`
3. `('command', 'New Worktree')`
4. `('fill', 'Branch name', 'transport-creation')`
5. `('fill', 'Base revision', 'HEAD')`
6. `('click', 'Create', 'button')`
7. `('peer_checkout', 'transport-creation')`
8. `('visible', 'Opening workspace')`
9. `('peer_transport', True)`
10. `('visible', 'Retry', 45)`
11. `('release_peer_checkout',)`
12. `('created_workspace', '/home/fixture/wt/project/transport-creation', True)`
13. `('peer_transport', False)`
14. `('click', 'Retry', 'button')`
15. `('absent', 'Opening workspace')`
16. `('type_terminal', 'pwd > /home/fixture/project/transport-result.txt')`
17. `('key', 'ENTER')`
18. `('remote_file_text', 'transport-result.txt', '/home/fixture/wt/project/transport-creation\n')`
19. `('peer_checkout', 'transport-creation')`

## CREATE-010: Local New Workspace opens one ready terminal at the user's home

Fixture: `isolated local installation`.

Behaviors: `creation`.

1. `('command', 'New Workspace')`
2. `('created_workspace', '.', False)`
3. `('run', '[ "$PWD" = "$HOME" ] && printf home-shell-ready > new-workspace.txt')`
4. `('home_file_text', 'new-workspace.txt', 'home-shell-ready')`
5. `('settle_count', 'workspaces', 2)`

## CREATE-011: A pending checkout prevents reordering until the new workspace is ready

Fixture: `isolated local installation`.

Behaviors: `creation`.

1. `('open_directory', 'Second workspace')`
2. `('created_workspace', 'Second workspace', False)`
3. `('click', 'Project ü with spaces', 'row')`
4. `('hold_worktree_checkout',)`
5. `('command', 'New Worktree')`
6. `('fill', 'Branch name', 'reorder-creation')`
7. `('fill', 'Base revision', 'HEAD')`
8. `('click', 'Create', 'button')`
9. `('checkout_pending',)`
10. `('visible', 'Opening workspace')`
11. `('drag', 'Second workspace', 'Project ü with spaces', 0.5, 0.12, 'row')`
12. `('workspace_prefix', 'Project ü with spaces', 'Second workspace')`
13. `('release_worktree_checkout',)`
14. `('created_workspace', 'wt/Project ü with spaces/reorder-creation', False)`
15. `('drag', 'Second workspace', 'Project ü with spaces', 0.5, 0.12, 'row')`
16. `('workspace_prefix', 'Second workspace', 'Project ü with spaces')`
17. `('settle_count', 'workspaces', 3)`
18. `('run', 'printf reordered-selection-ready > selected-workspace.txt')`
19. `('home_file_text', 'Second workspace/selected-workspace.txt', 'reordered-selection-ready')`

## UPDATE-SETTINGS-002: A slow Settings download shows one progress sheet and can be cancelled

Fixture: `update-slow-prepare`.

Behaviors: `update-settings`.

1. `('remember_layout',)`
2. `('remember_pids',)`
3. `('settings', 'General')`
4. `('click', 'Check for Updates…', 'button')`
5. `('visible', 'Jam 0.0.5 is available')`
6. `('enabled', 'Upgrade…', 'button')`
7. `('click', 'Upgrade…', 'button')`
8. `('visible', 'Downloading and verifying the update…')`
9. `('disabled', 'Upgrade and Restart', 'button')`
10. `('click', 'Cancel', 'button')`
11. `('absent', 'Upgrade Jam')`
12. `('installed_version', '0.0.4')`
13. `('same_layout',)`
14. `('same_pids',)`

## UPDATE-SETTINGS-003: Update preparation routes a foreign live server to a takeover confirmation

Fixture: `update-foreign-server`.

Behaviors: `update-settings`, `takeover`.

1. `('remember_layout',)`
2. `('remember_pids',)`
3. `('settings', 'General')`
4. `('click', 'Check for Updates…', 'button')`
5. `('visible', 'Jam 0.0.5 is available')`
6. `('enabled', 'Upgrade…', 'button')`
7. `('click', 'Upgrade…', 'button')`
8. `('visible', 'Take over the server on Local?', 60)`
9. `('visible', 'The server belongs to another Jam installation')`
10. `('enabled', 'Take Over and Reconnect', 'button')`
11. `('click', 'Cancel', 'button')`
12. `('same_layout',)`
13. `('same_pids',)`
14. `('installed_version', '0.0.4')`
