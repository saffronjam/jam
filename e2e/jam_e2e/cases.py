"""Reviewed product expectations, never populated from application implementation."""

from dataclasses import dataclass


@dataclass(frozen=True)
class Case:
    id: str
    title: str
    steps: tuple
    fixture: str | None = None


CASES = []


def case(id, title, *steps, fixture=None):
    CASES.append(Case(id, title, steps, fixture))


case(
    "TERM-001",
    "Shell executes a command",
    ("run", "printf 'shell-result'"),
    ("output", "shell-result"),
)
case("TERM-002", "Shell starts in the opened directory", ("run", "pwd"), ("output_repo",))
case(
    "TERM-003",
    "Unicode and combining text survives the terminal",
    ("run", "printf '日本語 café é 🚀'"),
    ("output", "日本語 café é 🚀"),
)
case(
    "TERM-004",
    "ANSI colors do not corrupt captured text",
    ("run", "printf '\\033[31mred-text\\033[0m'"),
    ("output", "red-text"),
)
case("TERM-005", "Large output reaches its final line", ("run", "seq 1 10000"), ("output", "10000"))
case(
    "TERM-009",
    "Accessible terminal text updates after a shell command",
    ("run", "printf '\\naccessible-output\\n'"),
    ("visible", "accessible-output"),
)
case(
    "TERM-006",
    "Terminal geometry reaches the PTY",
    ("run", "stty size"),
    ("output_regex", r"\b[1-9]\d* [1-9]\d*\b"),
)
case(
    "TERM-007",
    "Control-C interrupts a running program",
    ("type", "sleep 60"),
    ("key", "ENTER"),
    ("chord", "ctrl+c"),
    ("run", "printf 'after-interrupt'"),
    ("output", "after-interrupt"),
)
case(
    "TERM-008",
    "Alternate screen exits without losing history",
    ("run", "printf 'before-alt\\n\\033[?1049halternate\\033[?1049lafter-alt'"),
    ("output", "after-alt"),
    ("output", "before-alt"),
)

for i, text in enumerate(("ascii paste", "日本語 café", "spaces  and\ttabs", "first\nsecond"), 1):
    case(
        f"CLIP-{i:03}",
        f"Clipboard paste preserves {text!r}",
        ("clipboard", text),
        ("begin_input", "cat > pasted.txt"),
        ("paste",),
        ("key", "ENTER"),
        ("chord", "ctrl+d"),
        ("chord", "ctrl+d"),
        ("file_text", "pasted.txt", text + "\n"),
    )

for i, (title, kind, count) in enumerate(
    (
        ("New Tab", "tabs", 2),
        ("Split Right", "panes", 2),
        ("Split Down", "panes", 2),
        ("New Workspace", "workspaces", 2),
    ),
    1,
):
    case(
        f"NAV-{i:03}",
        title + " creates exactly one object",
        ("command", title),
        ("key", "ESCAPE"),
        ("count", kind, count),
    )
case(
    "TAB-001",
    "Closing a tab terminates its pane",
    ("command", "New Tab"),
    ("key", "ESCAPE"),
    ("context", "Tab 2"),
    ("click", "Close Tab"),
    ("count", "tabs", 1),
    ("count", "panes", 1),
)
case(
    "TAB-002",
    "Middle click closes a background tab",
    ("command", "New Tab"),
    ("key", "ESCAPE"),
    ("middle", "Tab 1"),
    ("count", "tabs", 1),
)
case(
    "TAB-003",
    "Renamed tab persists after reopening",
    ("context", "Tab 1"),
    ("click", "Rename"),
    ("type", "Review 日本語"),
    ("key", "ENTER"),
    ("relaunch",),
    ("visible", "Review 日本語"),
)
case(
    "TAB-004",
    "Escape cancels inline rename",
    ("context", "Tab 1"),
    ("click", "Rename"),
    ("type", "must-not-save"),
    ("key", "ESCAPE"),
    ("visible", "Tab 1"),
    ("absent", "must-not-save"),
)
case(
    "LIFE-001",
    "Closing and reopening the app retains the shell process",
    ("remember_pids",),
    ("relaunch",),
    ("same_pids",),
)
case(
    "LIFE-002",
    "Settings dialogs do not consume terminal input",
    ("settings", "Appearance"),
    ("type", "do-not-send-to-shell"),
    ("click", "Cancel"),
    ("no_output", "do-not-send-to-shell"),
)
case(
    "LIFE-003",
    "Daemon restart does not replay a command",
    ("run", "printf x >> executions"),
    ("restart_server",),
    ("file_text", "executions", "x"),
)
case(
    "LIFE-004",
    "Cancelled server restart leaves the shell alive",
    ("remember_pids",),
    ("context", "Local"),
    ("click", "Restart Server…"),
    ("click", "Cancel"),
    ("same_pids",),
)

TOGGLES = (
    ("General", "Close workspace after its last tab closes", "close_empty_workspaces", False),
    ("General", "Close tab after its last pane closes", "close_empty_tabs", True),
    ("General", "Allow deleting checkout files", "allow_workspace_deletion", True),
    ("General", "Copy text on selection", "copy_on_select", False),
    ("General", "Automatically check for updates", "automatic_check_updates", True),
    ("Coordination", "Automatically resume interrupted work", "resume_interrupted_work", True),
)
for index, (section, label, key, default) in enumerate(TOGGLES):
    for action in ("save", "cancel", "reopen"):
        steps = [("settings", section), ("click", label)]
        steps += [("click", "Cancel")] if action == "cancel" else [("save",)]
        if action == "reopen":
            steps.append(("relaunch",))
        steps += [
            (
                "config_default" if action == "cancel" else "config",
                key,
                default if action == "cancel" else not default,
            )
        ]
        case(
            f"CFG-{index * 3 + ('save', 'cancel', 'reopen').index(action) + 1:03}",
            f"{label}: {action}",
            *steps,
        )

for i, appearance in enumerate(("Light", "Dark", "System"), 19):
    case(
        f"CFG-{i:03}",
        f"Persist {appearance} appearance",
        ("settings", "Appearance"),
        ("click", appearance),
        ("save",),
        ("config", "appearance", appearance.lower()),
        ("relaunch",),
        ("config", "appearance", appearance.lower()),
    )
case(
    "CFG-022",
    "External edits invalidate an open draft",
    ("settings", "General"),
    ("write_config", "max_agents_per_coordinator = 3\n"),
    ("visible", "Settings changed outside this window"),
    ("disabled", "Save"),
    ("click", "Cancel"),
    ("config", "max_agents_per_coordinator", 3),
)
case(
    "CFG-023",
    "Reopening after a conflict loads the new settings",
    ("settings", "General"),
    ("write_config", 'appearance = "dark"\n'),
    ("visible", "Settings changed outside this window"),
    ("click", "Cancel"),
    ("settings", "Appearance"),
    ("save",),
    ("config", "appearance", "dark"),
)
for index, text in enumerate(
    (
        'appearance = "invalid"\n',
        "unknown_setting = true\n",
        "max_agents_per_coordinator = 0\n",
        'enabled_agents = ["unknown"]\n',
        '[bindings]\nunknown_action = ["ctrl+j"]\n',
        '[bindings]\nfind = ["f"]\n',
        '[bindings]\nfind = ["ctrl+j"]\npalette = ["ctrl+j"]\n',
        "broken = [\n",
    ),
    24,
):
    case(
        f"CFG-{index:03}",
        f"Malformed configuration produces a visible error ({index})",
        ("write_config", text),
        ("visible", "Could not load settings"),
        ("run", "printf still-running"),
    )
case(
    "CFG-032",
    "Saving through a symlink preserves it",
    ("config_symlink",),
    ("settings", "Appearance"),
    ("click", "Dark"),
    ("save",),
    ("config", "appearance", "dark"),
    ("symlink_preserved",),
)

for i, (title, chord, kind, total) in enumerate(
    (
        ("New Tab", "ctrl+shift+t", "tabs", 2),
        ("Split Right", "ctrl+shift+e", "panes", 2),
        ("Split Down", "ctrl+shift+o", "panes", 2),
        ("New Workspace", "ctrl+shift+n", "workspaces", 2),
    ),
    1,
):
    case(
        f"KEY-{i:03}",
        title + " via configured keyboard chord",
        ("chord", chord),
        ("key", "ESCAPE"),
        ("count", kind, total),
    )
case(
    "KEY-005",
    "A custom binding executes its command",
    ("write_config", '[bindings]\nnew_tab = ["ctrl+alt+j"]\n'),
    ("relaunch",),
    ("chord", "ctrl+alt+j"),
    ("key", "ESCAPE"),
    ("count", "tabs", 2),
)
case(
    "KEY-006",
    "An explicitly unbound command stays disabled after restart",
    ("write_config", "[bindings]\nnew_tab = []\n"),
    ("relaunch",),
    ("chord", "ctrl+shift+t"),
    ("settle_count", "tabs", 1),
)
case(
    "KEY-007",
    "Every configured binding is effective",
    ("write_config", '[bindings]\nnew_tab = ["ctrl+alt+j", "ctrl+alt+k"]\n'),
    ("relaunch",),
    ("chord", "ctrl+alt+j"),
    ("key", "ESCAPE"),
    ("count", "tabs", 2),
    ("chord", "ctrl+alt+k"),
    ("key", "ESCAPE"),
    ("count", "tabs", 3),
)

for i, query in enumerate(("needle", "日本語", "wrapped", "case"), 1):
    case(
        f"FIND-{i:03}",
        f"History search finds {query}",
        ("run", f"printf '{query}\\n'"),
        ("command", "Find in Tab History"),
        ("fill", "Find in tab history", query),
        ("visible", "1/1"),
    )
case(
    "FIND-005",
    "A search with no matches reports zero",
    ("command", "Find in Tab History"),
    ("fill", "Find in tab history", "never-produced-59104"),
    ("visible", "0/0"),
)
case(
    "FIND-006",
    "Escape returns keyboard focus to the terminal",
    ("command", "Find in Tab History"),
    ("key", "ESCAPE"),
    ("run", "printf after-find"),
    ("output", "after-find"),
)
case(
    "FIND-007",
    "Find follows a query across a soft-wrapped terminal line",
    (
        "run",
        "cols=$(stty size); cols=${cols##* }; printf '%*s%s\\n' \"$((cols-4))\" '' 'boundary-search'",
    ),
    ("command", "Find in Tab History"),
    ("fill", "Find in tab history", "boundary-search"),
    ("visible", "1/1"),
)

for i, (branch, base) in enumerate(
    (("feature/one", "HEAD"), ("日本語", "HEAD"), ("with-dashes", "main")), 1
):
    case(
        f"WT-{i:03}",
        f"Create worktree {branch} through the UI",
        ("command", "New Worktree"),
        ("fill", "Branch name", branch),
        ("fill", "Base revision", base),
        ("click", "Create"),
        ("count", "workspaces", 2),
        ("git_branch", branch),
    )
case(
    "WT-004",
    "Cancelling worktree creation preserves the repository",
    ("command", "New Worktree"),
    ("fill", "Branch name", "cancelled"),
    ("click", "Cancel"),
    ("settle_count", "workspaces", 1),
    ("git_no_branch", "cancelled"),
)
case(
    "WT-005",
    "Invalid base revision produces a recoverable error",
    ("command", "New Worktree"),
    ("fill", "Branch name", "bad-base"),
    ("fill", "Base revision", "no-such-ref"),
    ("click", "Create"),
    ("visible", "revision"),
    ("count", "workspaces", 1),
)

# These cases require external controlled services/artifacts. Their recipe creates
# only fixtures; each UI action and assertion remains in this independent suite.
DOMAINS = {
    "AGENT": [
        ("provider-missing", "Agent Integrations", "Not detected"),
        ("provider-detected", "Agent Integrations", "Install"),
        ("provider-installed", "Agent Integrations", "Installed"),
        ("provider-foreign", "Agent Integrations", "will be preserved"),
        ("provider-outdated", "Agent Integrations", "Update"),
    ],
    "COORD": [
        ("queued-task", "Settings", "Queued for delivery"),
        ("waiting-agent", "Settings", "Waiting for agent readiness"),
        ("accepted-task", "Settings", "awaiting acknowledgement"),
        ("acknowledged-task", "Settings", "Acknowledged by agent"),
        ("failed-task", "Settings", "Delivery failed"),
        ("completed-task", "Settings", "fixture-result"),
    ],
    "REMOTE": [
        ("remote-connected", None, "Connected"),
        ("remote-offline", None, "Offline"),
        ("remote-auth-rejected", None, "Permission denied"),
        ("remote-persistence-disabled", None, "Session persistence warning for Fixture"),
        ("remote-incompatible", None, "update"),
    ],
    "UPDATE": [
        ("update-available", None, "Upgrade…"),
        ("update-required", None, "update"),
        ("update-interrupted", None, "Recover…"),
    ],
}
for prefix, recipes in DOMAINS.items():
    for i, (recipe, command, expected) in enumerate(recipes, 1):
        steps = [("command", command)] if command else []
        if prefix == "COORD":
            steps += [("click", "Tasks and Agents")]
        case(
            f"{prefix}-{i:03}",
            recipe.replace("-", " "),
            *steps,
            ("visible", expected),
            fixture=recipe,
        )

assert len({c.id for c in CASES}) == len(CASES), "Duplicate case ID"

DEFAULTS = {
    "worktrees_directory": "~/wt/{{repo}}/{{worktree}}",
    "copy_on_select": False,
    "automatic_check_updates": True,
    "enabled_agents": ["claude", "codex"],
    "agent_models": {},
    "max_agents_per_coordinator": 8,
    "resume_interrupted_work": True,
    "appearance": "system",
    "close_empty_workspaces": False,
    "close_empty_tabs": True,
    "allow_workspace_deletion": True,
}
for i, (key, value) in enumerate(DEFAULTS.items(), 1):
    case(
        f"DEFAULT-{i:03}",
        f"Fresh profile uses documented {key}",
        ("settings", "General"),
        ("save",),
        ("config", key, value),
    )

BINDINGS = {
    "find": ("Find in Tab History", ["cmd+f", "ctrl+f"]),
    "find_next": ("Find Next", ["cmd+g"]),
    "find_previous": ("Find Previous", ["cmd+shift+g"]),
    "palette": ("Command Palette", ["ctrl+shift+p", "cmd+shift+p"]),
    "new_workspace": ("New Workspace", ["ctrl+shift+n"]),
    "open_workspace": ("Open Workspace", ["cmd+o"]),
    "new_tab": ("New Tab", ["ctrl+shift+t", "cmd+t"]),
    "split_right": ("Split Right", ["ctrl+shift+e", "cmd+d"]),
    "split_down": ("Split Down", ["ctrl+shift+o", "cmd+shift+d"]),
    "new_worktree": ("New Worktree", ["ctrl+shift+m"]),
    "open_worktree": ("Open Worktree", ["ctrl+shift+g"]),
    "close_pane": ("Close Pane", ["ctrl+shift+w"]),
    "close_workspace": ("Close Workspace", ["ctrl+shift+q"]),
    "settings": ("Settings", []),
    "agent_integrations": ("Agent Integrations", []),
}
for i, (key, (title, default)) in enumerate(BINDINGS.items()):
    for j, (name, value) in enumerate((("assign", ["ctrl+alt+j"]), ("unbind", []))):
        case(
            f"BIND-{i * 2 + j + 1:03}",
            f"{name} shortcut for {title} in settings",
            ("settings", "Keyboard"),
            ("fill", title, ", ".join(value)),
            ("save",),
            ("config", "bindings." + key, value),
            ("relaunch",),
            ("settings", "Keyboard"),
            ("field_value", title, ", ".join(value)),
        )

for i, provider in enumerate(("claude", "codex")):
    for j, (label, key) in enumerate(
        (
            ("Coordinator", "coordinator"),
            ("Worker", "worker"),
            ("Reviewer", "reviewer"),
            ("Complex work", "complex_work"),
        )
    ):
        case(
            f"MODEL-{i * 4 + j + 1:03}",
            f"Manual {provider} {label} model is persisted",
            ("settings", "Coordination"),
            ("click", "Claude" if provider == "claude" else "Codex"),
            ("fill", label + " model", "fixture-model-id"),
            ("save",),
            ("config", f"agent_models.{provider}.{key}", "fixture-model-id"),
        )

for i, (label, chord, expected) in enumerate(
    (
        ("letter", "x", "78"),
        ("tab", "TAB", "09"),
        ("enter", "ENTER", "0d"),
        ("escape", "ESCAPE", "1b"),
        ("control-a", "ctrl+a", "01"),
        ("control-e", "ctrl+e", "05"),
        ("control-z", "ctrl+z", "1a"),
        ("unicode", "日本語", "e697a5e69cace8aa9e"),
    ),
    1,
):
    case(
        f"INPUT-{i:03}",
        f"Physical input {label} reaches a raw PTY",
        ("probe",),
        ("probe_key", chord),
        ("probe_hex", expected),
    )

for i, (source, destination) in enumerate((("Tab 1", "Tab 2"), ("Tab 2", "Tab 1")), 1):
    case(
        f"DRAG-{i:03}",
        f"Drag {source} over {destination} preserves processes",
        ("command", "New Tab"),
        ("key", "ESCAPE"),
        ("remember_pids",),
        ("drag", source, destination),
        ("count", "tabs", 2),
        ("same_pids",),
    )

case(
    "INPUT-009",
    "A physical key press and release reach the PTY keyboard protocol",
    ("probe", "keyboard"),
    ("probe_key", "x"),
    ("probe_key_events",),
)
case(
    "MOUSE-001",
    "A physical click sends mouse press and release to the PTY",
    ("probe", "mouse"),
    ("click", "Terminal", "terminal"),
    ("probe_mouse_events",),
)

for i, (key, value, tabs, workspaces) in enumerate(
    (
        ("close_empty_tabs", False, 1, 1),
        ("close_empty_tabs", True, 0, 1),
        ("close_empty_workspaces", True, 0, 0),
    ),
    1,
):
    case(
        f"LIFECFG-{i:03}",
        f"{key}={value} controls the last pane lifecycle",
        ("write_config", f"{key} = {str(value).lower()}\n"),
        ("relaunch",),
        ("command", "Close Pane"),
        ("count", "panes", 0),
        ("count", "tabs", tabs),
        ("count", "workspaces", workspaces),
    )

for i, query in enumerate(("nothing", "Tab 1", "Split Right", "Settings", "Coordinator"), 1):
    case(
        f"PALETTE-{i:03}",
        f"Palette can be cancelled after searching {query}",
        ("chord", "ctrl+shift+p"),
        ("fill", "Search commands, workspaces, tabs, and panes", query),
        ("key", "ESCAPE"),
        ("run", "printf palette-returned-focus"),
        ("output", "palette-returned-focus"),
    )

# Full integration cases use prepared external worlds, then perform real UI actions.
INTEGRATION = [
    (
        "RECOVERY-001",
        "stopped-coordinator",
        [("click", "Recover Conversation…"), ("visible", "Recover Coordinator")],
    ),
    (
        "RECOVERY-002",
        "codex-conversations",
        [("click", "Recover Conversation…"), ("visible", "Jam coordinator")],
    ),
    (
        "RECOVERY-003",
        "codex-conversations",
        [
            ("click", "Recover Conversation…"),
            ("click", "Cancel"),
            ("visible", "Coordinator stopped"),
        ],
    ),
    (
        "RECOVERY-004",
        "stopped-coordinator",
        [("click", "Resume"), ("visible", "fixture-agent-resumed")],
    ),
    (
        "RECOVERY-005",
        "provider-launch-failed",
        [("visible", "Resume"), ("visible", "fixture-launch-error")],
    ),
    (
        "USAGE-001",
        "usage-two-providers",
        [("visible", "Codex"), ("visible", "Claude"), ("visible", "5h")],
    ),
    ("USAGE-002", "usage-weekly", [("visible", "7d")]),
    ("USAGE-003", "usage-unknown-account", [("visible", "Usage details"), ("visible", "Local")]),
    ("USAGE-004", "usage-no-sessions", [("absent", "USAGE")]),
    (
        "REMOTE-006",
        "remote-connected",
        [("context", "Fixture"), ("click", "Disconnect"), ("visible", "Disconnected")],
    ),
    (
        "REMOTE-007",
        "remote-connected",
        [
            ("context", "Fixture"),
            ("click", "Edit Remote…"),
            ("visible", "Save"),
            ("click", "Cancel"),
        ],
    ),
    (
        "REMOTE-008",
        "remote-connected",
        [("context", "Fixture"), ("click", "Remove Remote"), ("absent", "Fixture")],
    ),
    (
        "REMOTE-009",
        "remote-persistence-disabled",
        [("click", "Session persistence warning for Fixture"), ("visible", "Enable Persistence")],
    ),
    (
        "SERVER-001",
        "remote-update-available",
        [
            ("context", "Fixture"),
            ("click", "Update Server…"),
            ("visible", "Update When Sessions End"),
        ],
    ),
    (
        "SERVER-002",
        "remote-update-available",
        [
            ("context", "Fixture"),
            ("click", "Update Server…"),
            ("click", "Update When Sessions End"),
            ("context", "Fixture"),
            ("visible", "Cancel Scheduled Update"),
        ],
    ),
    (
        "SERVER-003",
        "remote-update-scheduled",
        [
            ("context", "Fixture"),
            ("click", "Cancel Scheduled Update"),
            ("context", "Fixture"),
            ("absent", "Cancel Scheduled Update"),
        ],
    ),
    (
        "SERVER-004",
        "remote-connected",
        [
            ("context", "Fixture"),
            ("click", "Restart Server…"),
            ("visible", "Running"),
            ("click", "Cancel"),
        ],
    ),
    (
        "UPDATE-004",
        "update-available",
        [
            ("click", "Skip This Version"),
            ("absent", "Upgrade…"),
            ("relaunch",),
            ("absent", "Upgrade…"),
        ],
    ),
    (
        "UPDATE-005",
        "update-available",
        [("click", "Upgrade…"), ("visible", "Upgrade and Restart"), ("click", "Cancel")],
    ),
    (
        "UPDATE-006",
        "update-interrupted",
        [("click", "Recover…"), ("visible", "Recover"), ("click", "Cancel")],
    ),
    (
        "UPDATE-007",
        "update-bad-signature",
        [("settings", "General"), ("click", "Check for Updates…"), ("visible", "signature")],
    ),
    ("UPDATE-008", "update-bad-checksum", [("click", "Upgrade…"), ("visible", "checksum")]),
    ("UPDATE-009", "update-download-failed", [("click", "Upgrade…"), ("visible", "download")]),
    (
        "UPDATE-010",
        "update-available",
        [
            ("click", "Upgrade…"),
            ("click", "Upgrade and Restart"),
            ("visible", "Open Release Notes"),
        ],
    ),
    (
        "WT-006",
        "dirty-checkout",
        [
            ("context", "Fixture checkout"),
            ("click", "Delete Checkout…"),
            ("visible", "Delete"),
            ("click", "Cancel"),
            ("file_exists", "../Fixture checkout/dirty.txt"),
        ],
    ),
    (
        "WT-007",
        "checkout-deletion-disabled",
        [("context", "Fixture checkout"), ("absent", "Delete Checkout…")],
    ),
    (
        "WT-008",
        "existing-checkouts",
        [
            ("command", "Open Worktree"),
            ("fill", "Filter by branch or path", "feature/existing"),
            ("visible", "feature/existing"),
        ],
    ),
    (
        "WT-009",
        "existing-checkouts",
        [
            ("command", "Open Worktree"),
            ("key", "ARROW_DOWN"),
            ("key", "ENTER"),
            ("count", "workspaces", 2),
        ],
    ),
    ("GIT-001", "git-diff", [("visible", "+3"), ("visible", "−2")]),
    ("GIT-002", "github-review", [("visible", "#42")]),
    ("GIT-003", "gitlab-review", [("visible", "!42")]),
    (
        "AGENT-006",
        "provider-detected",
        [
            ("command", "Agent Integrations"),
            ("click", "integration.claude.install"),
            ("visible", "Installed"),
        ],
    ),
    (
        "AGENT-007",
        "provider-installed",
        [
            ("command", "Agent Integrations"),
            ("click", "integration.claude.remove"),
            ("visible", "integration.claude.install"),
        ],
    ),
    (
        "AGENT-008",
        "provider-foreign",
        [
            ("command", "Agent Integrations"),
            ("visible", "will be preserved"),
            ("disabled", "integration.claude.install"),
            (
                "home_file_text",
                ".claude/skills/jam/SKILL.md",
                "User-owned foreign skill. Preserve these bytes.\n",
            ),
        ],
    ),
]
for id, recipe, steps in INTEGRATION:
    case(id, recipe.replace("-", " ") + " external user workflow", *steps, fixture=recipe)

for i, provider in enumerate(("Claude", "Codex"), 7):
    case(
        f"COORD-{i:03}",
        f"Start a {provider} coordinator through its workspace menu",
        ("context", "Fixture"),
        ("click", "Add Coordinator"),
        ("click", provider),
        ("visible", "fixture-agent-resumed"),
        ("agent_field", "provider", provider.lower()),
        fixture="provider-detected",
    )

case(
    "COORD-009",
    "Pause a coordinator through Tasks and Agents",
    ("settings", "Tasks and Agents"),
    ("click", "Pause"),
    ("visible", "Resume"),
    ("agent_field", "paused", True),
    fixture="single-coordinator",
)
case(
    "COORD-010",
    "Resume retains the conversation without submitting input",
    ("remember_agent_session",),
    ("remember_provider_inputs",),
    ("settings", "Tasks and Agents"),
    ("click", "Pause"),
    ("visible", "Resume"),
    ("click", "Resume"),
    ("agent_field", "paused", False),
    ("agent_field", "ready", True),
    ("same_agent_session",),
    ("provider_inputs_unchanged",),
    fixture="single-coordinator",
)
case(
    "COORD-011",
    "Instructions include the workspace's actual instruction file",
    ("settings", "Tasks and Agents"),
    ("click", "Instructions"),
    ("visible", "Fixture instruction: preserve user work."),
    fixture="single-coordinator",
)
case(
    "COORD-012",
    "Finished reviewed work is hidden until archived history is requested",
    ("settings", "Tasks and Agents"),
    ("absent", "Fixture task objective"),
    ("click", "Include archived"),
    ("visible", "Fixture task objective"),
    ("visible", "Archived · done"),
    fixture="archived-task",
)
