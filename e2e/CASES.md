# Desktop acceptance cases

212 cases; identical collection on macOS and Linux. No skips or expected failures.

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

1. `('type', 'sleep 60')`
2. `('key', 'ENTER')`
3. `('chord', 'ctrl+c')`
4. `('run', "printf 'after-interrupt'")`
5. `('output', 'after-interrupt')`

## TERM-008: Alternate screen exits without losing history

Fixture: `isolated local installation`.

1. `('run', "printf 'before-alt\\n\\033[?1049halternate\\033[?1049lafter-alt'")`
2. `('output', 'after-alt')`
3. `('output', 'before-alt')`

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
3. `('context', 'Tab 2')`
4. `('click', 'Close Tab')`
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

1. `('context', 'Tab 1')`
2. `('click', 'Rename')`
3. `('type', 'Review 日本語')`
4. `('key', 'ENTER')`
5. `('relaunch',)`
6. `('visible', 'Review 日本語')`

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
3. `('file_text', 'executions', 'x')`

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
4. `('key', 'ESCAPE')`
5. `('count', 'tabs', 2)`
6. `('chord', 'ctrl+alt+k')`
7. `('key', 'ESCAPE')`
8. `('count', 'tabs', 3)`

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
2. `('key', 'ESCAPE')`
3. `('run', 'printf after-find')`
4. `('output', 'after-find')`

## FIND-007: Find follows a query across a soft-wrapped terminal line

Fixture: `isolated local installation`.

1. `('run', 'cols=$(stty size); cols=${cols##* }; printf \'%*s%s\\n\' "$((cols-4))" \'\' \'boundary-search\'')`
2. `('command', 'Find in Tab History')`
3. `('fill', 'Find in tab history', 'boundary-search')`
4. `('visible', '1/1')`

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

1. `('command', 'New Worktree')`
2. `('fill', 'Branch name', 'bad-base')`
3. `('fill', 'Base revision', 'no-such-ref')`
4. `('click', 'Create')`
5. `('visible', 'revision')`
6. `('count', 'workspaces', 1)`

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

1. `('visible', 'Connected')`

## REMOTE-002: remote offline

Fixture: `remote-offline`.

1. `('visible', 'Offline')`

## REMOTE-003: remote auth rejected

Fixture: `remote-auth-rejected`.

1. `('visible', 'Permission denied')`

## REMOTE-004: remote persistence disabled

Fixture: `remote-persistence-disabled`.

1. `('visible', 'Session persistence warning for Fixture')`

## REMOTE-005: remote incompatible

Fixture: `remote-incompatible`.

1. `('visible', 'update')`

## UPDATE-001: update available

Fixture: `update-available`.

1. `('visible', 'Upgrade…')`

## UPDATE-002: update required

Fixture: `update-required`.

1. `('visible', 'Upgrade…')`
2. `('click', 'Upgrade…')`
3. `('visible', 'The remote machine needs Jam 0.0.5 or newer before connecting.')`
4. `('click', 'Cancel')`
5. `('remote_workspace_preserved',)`

## UPDATE-003: update interrupted

Fixture: `update-interrupted`.

1. `('visible', 'Recover…')`

## REMOTE-010: Cancel real SSH continue

Fixture: `remote-password-required`.

1. `('context', 'Fixture')`
2. `('click', 'Reconnect')`
3. `('visible', 'SSH Authentication')`
4. `('click', 'Cancel', 'button')`
5. `('absent', 'SSH Authentication')`
6. `('visible', 'Permission denied')`

## REMOTE-011: Accept real SSH continue

Fixture: `remote-password-required`.

1. `('context', 'Fixture')`
2. `('click', 'Reconnect')`
3. `('visible', 'SSH Authentication')`
4. `('type', 'fixture-local-only')`
5. `('click', 'Continue', 'button')`
6. `('visible', 'Connected')`
7. `('remote_workspace_preserved',)`

## REMOTE-012: Cancel real SSH trust host

Fixture: `remote-host-untrusted`.

1. `('context', 'Fixture')`
2. `('click', 'Reconnect')`
3. `('visible', 'SSH Authentication')`
4. `('click', 'Cancel', 'button')`
5. `('absent', 'SSH Authentication')`
6. `('visible', 'Host key verification failed')`

## REMOTE-013: Accept real SSH trust host

Fixture: `remote-host-untrusted`.

1. `('context', 'Fixture')`
2. `('click', 'Reconnect')`
3. `('visible', 'SSH Authentication')`
4. `('click', 'Trust Host', 'button')`
5. `('visible', 'Connected')`
6. `('remote_workspace_preserved',)`

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
3. `('save',)`
4. `('config', 'bindings.find', ['ctrl+alt+j'])`
5. `('relaunch',)`
6. `('settings', 'Keyboard')`
7. `('field_value', 'Find in Tab History', 'ctrl+alt+j')`

## BIND-002: unbind shortcut for Find in Tab History in settings

Fixture: `isolated local installation`.

1. `('settings', 'Keyboard')`
2. `('fill', 'Find in Tab History', '')`
3. `('save',)`
4. `('config', 'bindings.find', [])`
5. `('relaunch',)`
6. `('settings', 'Keyboard')`
7. `('field_value', 'Find in Tab History', '')`

## BIND-003: assign shortcut for Find Next in settings

Fixture: `isolated local installation`.

1. `('settings', 'Keyboard')`
2. `('fill', 'Find Next', 'ctrl+alt+j')`
3. `('save',)`
4. `('config', 'bindings.find_next', ['ctrl+alt+j'])`
5. `('relaunch',)`
6. `('settings', 'Keyboard')`
7. `('field_value', 'Find Next', 'ctrl+alt+j')`

## BIND-004: unbind shortcut for Find Next in settings

Fixture: `isolated local installation`.

1. `('settings', 'Keyboard')`
2. `('fill', 'Find Next', '')`
3. `('save',)`
4. `('config', 'bindings.find_next', [])`
5. `('relaunch',)`
6. `('settings', 'Keyboard')`
7. `('field_value', 'Find Next', '')`

## BIND-005: assign shortcut for Find Previous in settings

Fixture: `isolated local installation`.

1. `('settings', 'Keyboard')`
2. `('fill', 'Find Previous', 'ctrl+alt+j')`
3. `('save',)`
4. `('config', 'bindings.find_previous', ['ctrl+alt+j'])`
5. `('relaunch',)`
6. `('settings', 'Keyboard')`
7. `('field_value', 'Find Previous', 'ctrl+alt+j')`

## BIND-006: unbind shortcut for Find Previous in settings

Fixture: `isolated local installation`.

1. `('settings', 'Keyboard')`
2. `('fill', 'Find Previous', '')`
3. `('save',)`
4. `('config', 'bindings.find_previous', [])`
5. `('relaunch',)`
6. `('settings', 'Keyboard')`
7. `('field_value', 'Find Previous', '')`

## BIND-007: assign shortcut for Command Palette in settings

Fixture: `isolated local installation`.

1. `('settings', 'Keyboard')`
2. `('fill', 'Command Palette', 'ctrl+alt+j')`
3. `('save',)`
4. `('config', 'bindings.palette', ['ctrl+alt+j'])`
5. `('relaunch',)`
6. `('settings', 'Keyboard')`
7. `('field_value', 'Command Palette', 'ctrl+alt+j')`

## BIND-008: unbind shortcut for Command Palette in settings

Fixture: `isolated local installation`.

1. `('settings', 'Keyboard')`
2. `('fill', 'Command Palette', '')`
3. `('save',)`
4. `('config', 'bindings.palette', [])`
5. `('relaunch',)`
6. `('settings', 'Keyboard')`
7. `('field_value', 'Command Palette', '')`

## BIND-009: assign shortcut for New Workspace in settings

Fixture: `isolated local installation`.

1. `('settings', 'Keyboard')`
2. `('fill', 'New Workspace', 'ctrl+alt+j')`
3. `('save',)`
4. `('config', 'bindings.new_workspace', ['ctrl+alt+j'])`
5. `('relaunch',)`
6. `('settings', 'Keyboard')`
7. `('field_value', 'New Workspace', 'ctrl+alt+j')`

## BIND-010: unbind shortcut for New Workspace in settings

Fixture: `isolated local installation`.

1. `('settings', 'Keyboard')`
2. `('fill', 'New Workspace', '')`
3. `('save',)`
4. `('config', 'bindings.new_workspace', [])`
5. `('relaunch',)`
6. `('settings', 'Keyboard')`
7. `('field_value', 'New Workspace', '')`

## BIND-011: assign shortcut for Open Workspace in settings

Fixture: `isolated local installation`.

1. `('settings', 'Keyboard')`
2. `('fill', 'Open Workspace', 'ctrl+alt+j')`
3. `('save',)`
4. `('config', 'bindings.open_workspace', ['ctrl+alt+j'])`
5. `('relaunch',)`
6. `('settings', 'Keyboard')`
7. `('field_value', 'Open Workspace', 'ctrl+alt+j')`

## BIND-012: unbind shortcut for Open Workspace in settings

Fixture: `isolated local installation`.

1. `('settings', 'Keyboard')`
2. `('fill', 'Open Workspace', '')`
3. `('save',)`
4. `('config', 'bindings.open_workspace', [])`
5. `('relaunch',)`
6. `('settings', 'Keyboard')`
7. `('field_value', 'Open Workspace', '')`

## BIND-013: assign shortcut for New Tab in settings

Fixture: `isolated local installation`.

1. `('settings', 'Keyboard')`
2. `('fill', 'New Tab', 'ctrl+alt+j')`
3. `('save',)`
4. `('config', 'bindings.new_tab', ['ctrl+alt+j'])`
5. `('relaunch',)`
6. `('settings', 'Keyboard')`
7. `('field_value', 'New Tab', 'ctrl+alt+j')`

## BIND-014: unbind shortcut for New Tab in settings

Fixture: `isolated local installation`.

1. `('settings', 'Keyboard')`
2. `('fill', 'New Tab', '')`
3. `('save',)`
4. `('config', 'bindings.new_tab', [])`
5. `('relaunch',)`
6. `('settings', 'Keyboard')`
7. `('field_value', 'New Tab', '')`

## BIND-015: assign shortcut for Split Right in settings

Fixture: `isolated local installation`.

1. `('settings', 'Keyboard')`
2. `('fill', 'Split Right', 'ctrl+alt+j')`
3. `('save',)`
4. `('config', 'bindings.split_right', ['ctrl+alt+j'])`
5. `('relaunch',)`
6. `('settings', 'Keyboard')`
7. `('field_value', 'Split Right', 'ctrl+alt+j')`

## BIND-016: unbind shortcut for Split Right in settings

Fixture: `isolated local installation`.

1. `('settings', 'Keyboard')`
2. `('fill', 'Split Right', '')`
3. `('save',)`
4. `('config', 'bindings.split_right', [])`
5. `('relaunch',)`
6. `('settings', 'Keyboard')`
7. `('field_value', 'Split Right', '')`

## BIND-017: assign shortcut for Split Down in settings

Fixture: `isolated local installation`.

1. `('settings', 'Keyboard')`
2. `('fill', 'Split Down', 'ctrl+alt+j')`
3. `('save',)`
4. `('config', 'bindings.split_down', ['ctrl+alt+j'])`
5. `('relaunch',)`
6. `('settings', 'Keyboard')`
7. `('field_value', 'Split Down', 'ctrl+alt+j')`

## BIND-018: unbind shortcut for Split Down in settings

Fixture: `isolated local installation`.

1. `('settings', 'Keyboard')`
2. `('fill', 'Split Down', '')`
3. `('save',)`
4. `('config', 'bindings.split_down', [])`
5. `('relaunch',)`
6. `('settings', 'Keyboard')`
7. `('field_value', 'Split Down', '')`

## BIND-019: assign shortcut for New Worktree in settings

Fixture: `isolated local installation`.

1. `('settings', 'Keyboard')`
2. `('fill', 'New Worktree', 'ctrl+alt+j')`
3. `('save',)`
4. `('config', 'bindings.new_worktree', ['ctrl+alt+j'])`
5. `('relaunch',)`
6. `('settings', 'Keyboard')`
7. `('field_value', 'New Worktree', 'ctrl+alt+j')`

## BIND-020: unbind shortcut for New Worktree in settings

Fixture: `isolated local installation`.

1. `('settings', 'Keyboard')`
2. `('fill', 'New Worktree', '')`
3. `('save',)`
4. `('config', 'bindings.new_worktree', [])`
5. `('relaunch',)`
6. `('settings', 'Keyboard')`
7. `('field_value', 'New Worktree', '')`

## BIND-021: assign shortcut for Open Worktree in settings

Fixture: `isolated local installation`.

1. `('settings', 'Keyboard')`
2. `('fill', 'Open Worktree', 'ctrl+alt+j')`
3. `('save',)`
4. `('config', 'bindings.open_worktree', ['ctrl+alt+j'])`
5. `('relaunch',)`
6. `('settings', 'Keyboard')`
7. `('field_value', 'Open Worktree', 'ctrl+alt+j')`

## BIND-022: unbind shortcut for Open Worktree in settings

Fixture: `isolated local installation`.

1. `('settings', 'Keyboard')`
2. `('fill', 'Open Worktree', '')`
3. `('save',)`
4. `('config', 'bindings.open_worktree', [])`
5. `('relaunch',)`
6. `('settings', 'Keyboard')`
7. `('field_value', 'Open Worktree', '')`

## BIND-023: assign shortcut for Close Pane in settings

Fixture: `isolated local installation`.

1. `('settings', 'Keyboard')`
2. `('fill', 'Close Pane', 'ctrl+alt+j')`
3. `('save',)`
4. `('config', 'bindings.close_pane', ['ctrl+alt+j'])`
5. `('relaunch',)`
6. `('settings', 'Keyboard')`
7. `('field_value', 'Close Pane', 'ctrl+alt+j')`

## BIND-024: unbind shortcut for Close Pane in settings

Fixture: `isolated local installation`.

1. `('settings', 'Keyboard')`
2. `('fill', 'Close Pane', '')`
3. `('save',)`
4. `('config', 'bindings.close_pane', [])`
5. `('relaunch',)`
6. `('settings', 'Keyboard')`
7. `('field_value', 'Close Pane', '')`

## BIND-025: assign shortcut for Close Workspace in settings

Fixture: `isolated local installation`.

1. `('settings', 'Keyboard')`
2. `('fill', 'Close Workspace', 'ctrl+alt+j')`
3. `('save',)`
4. `('config', 'bindings.close_workspace', ['ctrl+alt+j'])`
5. `('relaunch',)`
6. `('settings', 'Keyboard')`
7. `('field_value', 'Close Workspace', 'ctrl+alt+j')`

## BIND-026: unbind shortcut for Close Workspace in settings

Fixture: `isolated local installation`.

1. `('settings', 'Keyboard')`
2. `('fill', 'Close Workspace', '')`
3. `('save',)`
4. `('config', 'bindings.close_workspace', [])`
5. `('relaunch',)`
6. `('settings', 'Keyboard')`
7. `('field_value', 'Close Workspace', '')`

## BIND-027: assign shortcut for Settings in settings

Fixture: `isolated local installation`.

1. `('settings', 'Keyboard')`
2. `('fill', 'Settings', 'ctrl+alt+j')`
3. `('save',)`
4. `('config', 'bindings.settings', ['ctrl+alt+j'])`
5. `('relaunch',)`
6. `('settings', 'Keyboard')`
7. `('field_value', 'Settings', 'ctrl+alt+j')`

## BIND-028: unbind shortcut for Settings in settings

Fixture: `isolated local installation`.

1. `('settings', 'Keyboard')`
2. `('fill', 'Settings', '')`
3. `('save',)`
4. `('config', 'bindings.settings', [])`
5. `('relaunch',)`
6. `('settings', 'Keyboard')`
7. `('field_value', 'Settings', '')`

## BIND-029: assign shortcut for Agent Integrations in settings

Fixture: `isolated local installation`.

1. `('settings', 'Keyboard')`
2. `('fill', 'Agent Integrations', 'ctrl+alt+j')`
3. `('save',)`
4. `('config', 'bindings.agent_integrations', ['ctrl+alt+j'])`
5. `('relaunch',)`
6. `('settings', 'Keyboard')`
7. `('field_value', 'Agent Integrations', 'ctrl+alt+j')`

## BIND-030: unbind shortcut for Agent Integrations in settings

Fixture: `isolated local installation`.

1. `('settings', 'Keyboard')`
2. `('fill', 'Agent Integrations', '')`
3. `('save',)`
4. `('config', 'bindings.agent_integrations', [])`
5. `('relaunch',)`
6. `('settings', 'Keyboard')`
7. `('field_value', 'Agent Integrations', '')`

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
4. `('run', 'printf palette-returned-focus')`
5. `('output', 'palette-returned-focus')`

## PALETTE-002: Palette can be cancelled after searching Tab 1

Fixture: `isolated local installation`.

1. `('chord', 'ctrl+shift+p')`
2. `('fill', 'Search commands, workspaces, tabs, and panes', 'Tab 1')`
3. `('key', 'ESCAPE')`
4. `('run', 'printf palette-returned-focus')`
5. `('output', 'palette-returned-focus')`

## PALETTE-003: Palette can be cancelled after searching Split Right

Fixture: `isolated local installation`.

1. `('chord', 'ctrl+shift+p')`
2. `('fill', 'Search commands, workspaces, tabs, and panes', 'Split Right')`
3. `('key', 'ESCAPE')`
4. `('run', 'printf palette-returned-focus')`
5. `('output', 'palette-returned-focus')`

## PALETTE-004: Palette can be cancelled after searching Settings

Fixture: `isolated local installation`.

1. `('chord', 'ctrl+shift+p')`
2. `('fill', 'Search commands, workspaces, tabs, and panes', 'Settings')`
3. `('key', 'ESCAPE')`
4. `('run', 'printf palette-returned-focus')`
5. `('output', 'palette-returned-focus')`

## PALETTE-005: Palette can be cancelled after searching Coordinator

Fixture: `isolated local installation`.

1. `('chord', 'ctrl+shift+p')`
2. `('fill', 'Search commands, workspaces, tabs, and panes', 'Coordinator')`
3. `('key', 'ESCAPE')`
4. `('run', 'printf palette-returned-focus')`
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

1. `('visible', 'Usage details')`
2. `('visible', 'Local')`

## USAGE-004: usage no sessions external user workflow

Fixture: `usage-no-sessions`.

1. `('absent', 'USAGE')`

## REMOTE-006: remote connected external user workflow

Fixture: `remote-connected`.

1. `('context', 'Fixture')`
2. `('click', 'Disconnect')`
3. `('visible', 'Disconnected')`

## REMOTE-007: remote connected external user workflow

Fixture: `remote-connected`.

1. `('context', 'Fixture')`
2. `('click', 'Edit Remote…')`
3. `('visible', 'Save')`
4. `('click', 'Cancel')`

## REMOTE-008: remote connected external user workflow

Fixture: `remote-connected`.

1. `('context', 'Fixture')`
2. `('click', 'Remove Remote')`
3. `('absent', 'Fixture')`

## REMOTE-009: remote persistence disabled external user workflow

Fixture: `remote-persistence-disabled`.

1. `('click', 'Session persistence warning for Fixture')`
2. `('visible', 'Enable Persistence')`

## SERVER-001: remote update available external user workflow

Fixture: `remote-update-available`.

1. `('context', 'Fixture')`
2. `('click', 'Update Server…')`
3. `('visible', 'Update When Sessions End')`

## SERVER-002: remote update available external user workflow

Fixture: `remote-update-available`.

1. `('context', 'Fixture')`
2. `('click', 'Update Server…')`
3. `('click', 'Update When Sessions End')`
4. `('context', 'Fixture')`
5. `('visible', 'Cancel Scheduled Update')`

## SERVER-003: remote update available external user workflow

Fixture: `remote-update-available`.

1. `('context', 'Fixture')`
2. `('click', 'Update Server…')`
3. `('click', 'Update When Sessions End')`
4. `('context', 'Fixture')`
5. `('click', 'Cancel Scheduled Update')`
6. `('context', 'Fixture')`
7. `('absent', 'Cancel Scheduled Update')`

## SERVER-004: remote connected external user workflow

Fixture: `remote-connected`.

1. `('context', 'Fixture')`
2. `('click', 'Restart Server…')`
3. `('visible', 'Running')`
4. `('click', 'Cancel')`

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

1. `('click', 'Upgrade…')`
2. `('click', 'Details', 'disclosure')`
3. `('visible', 'download')`

## UPDATE-010: update available external user workflow

Fixture: `update-available`.

1. `('run', 'printf x >> update-executions')`
2. `('remember_layout',)`
3. `('click', 'Upgrade…')`
4. `('click', 'Upgrade and Restart')`
5. `('installed_version', '0.0.5')`
6. `('visible', 'Open Release Notes')`
7. `('same_layout',)`
8. `('file_text', 'update-executions', 'x')`

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
2. `('key', 'ARROW_DOWN')`
3. `('key', 'ENTER')`
4. `('count', 'workspaces', 2)`

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
