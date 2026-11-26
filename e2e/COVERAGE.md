# New-main behavior coverage

Scope: `ac5d161..d7b775c`. Runtime proof is recorded separately.

Unreviewed mappings are incomplete, even when some scenarios exist.

| Behavior | Review | Shared cases |
|---|---|---|
| Native image paste and errors | Reviewed | IMAGE-001, IMAGE-002, IMAGE-003, IMAGE-004, IMAGE-005, IMAGE-006, IMAGE-007, IMAGE-008 |
| Pending workspace and worktree operations | Reviewed | WT-008, WT-009, CREATE-001, CREATE-002, CREATE-003, CREATE-004, CREATE-005, CREATE-006, CREATE-007, CREATE-008, CREATE-009, CREATE-010, CREATE-011, CREATE-012, CREATE-013, CREATE-014, CREATE-015 |
| Creation interruption and compatibility | Reviewed | CREATE-003, CREATE-005, CREATE-009, CREATE-012, CREATE-013 |
| Explicit server takeover and rollback | Reviewed | TAKEOVER-001, TAKEOVER-002, TAKEOVER-003, TAKEOVER-004, TAKEOVER-005, TAKEOVER-006, TAKEOVER-007, UPDATE-SETTINGS-003 |
| Timeout diagnostics and server operations | Reviewed | REMOTE-002, REMOTE-003, DIAG-001, DIAG-002, SIDEBAR-001 |
| Nested settings update flows | Reviewed | UPDATE-004, UPDATE-005, UPDATE-006, UPDATE-007, UPDATE-008, UPDATE-009, UPDATE-010, UPDATE-011, UPDATE-012, UPDATE-SETTINGS-001, TAKEOVER-003, UPDATE-SETTINGS-002, UPDATE-SETTINGS-003, UPDATE-SETTINGS-004, UPDATE-SETTINGS-005, UPDATE-SETTINGS-006, UPDATE-SETTINGS-007 |
| Account grouping and source availability | Reviewed | USAGE-001, USAGE-002, USAGE-003, USAGE-004, USAGE-005, USAGE-006, ACCOUNT-001, ACCOUNT-002, ACCOUNT-003, ACCOUNT-004, ACCOUNT-005, ACCOUNT-006, ACCOUNT-007 |
| Reported agent identity and lifetime | Reviewed | IDENTITY-001, IDENTITY-002, IDENTITY-003, IDENTITY-004, IDENTITY-005, IMAGE-004, FOCUS-001, FOCUS-002 |
| Focus acknowledgement and close confirmations | Reviewed | CLOSE-001, CLOSE-002, CLOSE-003, CLOSE-004, FOCUS-001, FOCUS-002, RENAME-001, CREATE-015 |
| Synchronized frames, cursor and sidebar presentation | Reviewed | LONG-001, VISUAL-001, VISUAL-002, VISUAL-003, RENDER-001, RENDER-002, RENDER-003, SIDEBAR-001, RENAME-001 |
| Themes and default terminal selection | Reviewed | THEME-001, SELECT-003 |
| Optimistic layout and pane focus | Reviewed | LAYOUT-003, LAYOUT-004, LAYOUT-005 |
| Terminal geometry during layout transitions | Reviewed | LAYOUT-003 |
| Continuity for manually started agents | Reviewed | RECOVERY-006, RECOVERY-007 |
| Usage readings and rate-limit backoff across restarts | Reviewed | USAGE-007, USAGE-008 |
| Upgrade toolbar and expiring notifications | Reviewed | UPDATE-014, UPDATE-015 |
