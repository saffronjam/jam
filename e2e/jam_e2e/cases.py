"""Reviewed product expectations, never populated from application implementation."""

from dataclasses import dataclass


@dataclass(frozen=True)
class Case:
    id: str
    title: str
    steps: tuple
    fixture: str | None = None
    behavior_ids: tuple[str, ...] = ()


CASES = []


def case(id, title, *steps, fixture=None, behavior_ids=()):
    CASES.append(Case(id, title, steps, fixture, tuple(behavior_ids)))


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
    # Emit exact UTF-8 bytes through ASCII shell input. Physical text entry has
    # separate INPUT cases, including ordinary clipboard paste below.
    ("run", "printf '" + "".join(f"\\{byte:03o}" for byte in "日本語 café é 🚀".encode()) + "'"),
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
    ("interrupt_probe",),
    ("chord", "ctrl+c"),
    ("interrupted",),
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

for index, layout in enumerate(("French", "German"), 1):
    case(
        f"LAYOUT-{index:03}",
        f"Native {layout} input preserves text, punctuation and shortcuts",
        ("input_source", layout),
        ("run", "printf '%s' '[azerty] {123}: y-z'"),
        ("output", "[azerty] {123}: y-z"),
        ("chord", "ctrl+shift+t"),
        ("count", "tabs", 2),
        ("run", "printf layout-after-shortcut"),
        ("output", "layout-after-shortcut"),
        ("restore_input_source",),
    )

case(
    "SCALE-001",
    "Native high-density output preserves pixels, hit testing and PTY geometry",
    ("resize", 1000, 660),
    ("remember_layout",),
    ("remember_pids",),
    ("run", "printf '\\116\\101\\124\\111\\126\\105 SCALE 7429'"),
    ("visual_text", "NATIVE SCALE 7429"),
    ("display_scale", "high"),
    ("native_scale", "high"),
    ("visual_text", "NATIVE SCALE 7429"),
    ("geometry_probe",),
    ("resize", 900, 580),
    ("geometry_changed",),
    ("run", "printf '\\123\\103\\101\\114\\105\\104 INPUT 1954'"),
    ("visual_text", "SCALED INPUT 1954"),
    ("same_pids",),
    ("same_layout",),
    ("display_scale", "normal"),
    ("native_scale", "normal"),
    ("run", "printf '\\122\\105\\123\\124\\117\\122\\105\\104 SCALE 8263'"),
    ("visual_text", "RESTORED SCALE 8263"),
)

case(
    "MOTION-001",
    "Native reduced motion preserves navigation, modal focus and terminal input",
    ("native_reduced_motion",),
    ("count", "workspaces", 1),
    ("remember_layout",),
    ("remember_pids",),
    ("chord", "ctrl+shift+p"),
    ("target_visible", "Search commands, workspaces, tabs, and panes", "field"),
    ("key", "ESCAPE"),
    ("settings", "Appearance"),
    ("click", "Cancel", "button"),
    ("run", "printf '\\115\\117\\124\\111\\117\\116 READY 4638'"),
    ("visual_text", "MOTION READY 4638"),
    ("same_layout",),
    ("same_pids",),
    fixture="native-reduced-motion",
)

for i, text in enumerate(
    ("ascii paste", "日本語 café", "spaces  and\ttabs", "first\nsecond", "é 🚀 👨‍👩‍👧‍👦"), 1
):
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
    ("context", "Tab 2", "button"),
    ("click", "Close Tab", "menu"),
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
    ("clipboard", "Review 日本語"),
    ("context", "Tab 1"),
    ("click", "Rename"),
    ("fill", "Tab name", "Review 日本語"),
    ("field_value", "Tab name", "Review 日本語"),
    ("key", "ENTER"),
    ("tab_name", "Review 日本語"),
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
    ("file_text", "executions", "x", 2),
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
        steps = [("settings", section), ("click", label, "toggle")]
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
    ("target_visible", "Tab name", "field"),
    ("key", "ESCAPE"),
    ("count", "tabs", 2),
    ("chord", "ctrl+alt+k"),
    ("target_visible", "Tab name", "field"),
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
    ("target_visible", "Find in tab history", "field"),
    ("key", "ESCAPE"),
    ("absent", "Find in tab history"),
    ("run_focused", "printf after-find"),
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

case(
    "RACE-001",
    "Rapid history queries settle on the final query without later stale counts",
    ("run", "seq 1 6000 | sed 's/^/racealpha-/'; printf '\\nfinal-racebeta\\n'"),
    ("remember_layout",),
    ("remember_pids",),
    ("command", "Find in Tab History"),
    ("query_burst", "Find in tab history", ("racealpha", "no-match-1973", "racealpha", "final-racebeta")),
    ("field_value", "Find in tab history", "final-racebeta"),
    ("stable_visible", "1/1", 2),
    ("key", "ESCAPE"),
    ("absent", "Find in tab history"),
    ("run_focused", "printf search-final-query-focus"),
    ("output", "search-final-query-focus"),
    ("same_layout",),
    ("same_pids",),
)
case(
    "RACE-002",
    "Dismissing rapidly replaced history queries keeps search closed and restores terminal focus",
    ("run", "seq 1 6000 | sed 's/^/cancel-races-/'"),
    ("remember_layout",),
    ("remember_pids",),
    ("command", "Find in Tab History"),
    ("query_burst", "Find in tab history", ("cancel-races", "never-matches-8249", "cancel-races")),
    ("key", "ESCAPE"),
    ("absent", "Find in tab history"),
    ("run_focused", "printf cancelled-search-focus"),
    ("output", "cancelled-search-focus"),
    ("absent", "Find in tab history"),
    ("same_layout",),
    ("same_pids",),
)
case(
    "RACE-003",
    "History results follow the selected tab after rapid query replacement",
    ("run", "printf scope-first-tab"),
    ("command", "New Tab"),
    ("key", "ESCAPE"),
    ("count", "tabs", 2),
    ("run", "printf scope-second-tab"),
    ("remember_layout",),
    ("remember_pids",),
    ("command", "Find in Tab History"),
    ("query_burst", "Find in tab history", ("scope-first-tab", "no-match-5917", "scope-second-tab")),
    ("click", "Tab 1", "button"),
    ("field_value", "Find in tab history", "scope-second-tab"),
    ("stable_visible", "0/0", 2),
    ("click", "Tab 2", "button"),
    ("stable_visible", "1/1", 2),
    ("key", "ESCAPE"),
    ("absent", "Find in tab history"),
    ("run_focused", "printf search-scope-focus"),
    ("output", "search-scope-focus"),
    ("same_layout",),
    ("same_pids",),
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
    ("remember_layout",),
    ("remember_pids",),
    ("command", "New Worktree"),
    ("fill", "Branch name", "bad-base"),
    ("fill", "Base revision", "no-such-ref"),
    ("click", "Create"),
    ("click", "Details", "button"),
    ("visible", "no-such-ref"),
    ("count", "workspaces", 1),
    ("same_layout",),
    ("same_pids",),
    ("git_no_branch", "bad-base"),
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
        ("remote-offline", None, "Server unavailable"),
        ("remote-auth-rejected", None, "Permission denied"),
        ("remote-persistence-disabled", None, "Session persistence warning for Fixture"),
        ("remote-incompatible", None, "Takeover required"),
    ],
    "UPDATE": [
        ("update-available", None, "Upgrade…"),
        ("update-required", None, "Upgrade…"),
        ("update-interrupted", None, "Recover…"),
    ],
}
for prefix, recipes in DOMAINS.items():
    for i, (recipe, command, expected) in enumerate(recipes, 1):
        steps = [("command", command)] if command else []
        if prefix == "COORD":
            steps += [("click", "Tasks and Agents")]
        # The first real SSH connection can include hosted local-network
        # consent and several TCG guest probes before its warning appears.
        steps += [("visible", expected, 60)] if recipe == "remote-persistence-disabled" else [("visible", expected)]
        if recipe == "remote-connected":
            steps = [("remote_shell",), ("remote_workspace_preserved",)]
        elif recipe == "remote-auth-rejected":
            steps = [
                ("visible", "Server unavailable"), ("absent", "SSH Authentication"),
                ("remote_rejected", "Permission denied"),
                ("context", "Fixture", "machine"), ("enabled", "Reconnect", "menu"),
                ("key", "ESCAPE"), ("remote_workspace_preserved",),
            ]
        if recipe == "update-required":
            steps += [
                ("click", "Upgrade…"),
                ("visible", "Upgrade Jam"),
                ("visible", "Jam 0.0.5"),
                ("visible", "Fixture"),
                ("visible", "1 running terminal"),
                ("click", "Cancel", "button"),
                ("remote_workspace_preserved",),
            ]
        behavior_ids = ("diagnostics",) if recipe in {"remote-offline", "remote-auth-rejected"} else ()
        case(
            f"{prefix}-{i:03}",
            recipe.replace("-", " "),
            *steps,
            fixture=recipe,
            behavior_ids=behavior_ids,
        )

for id, recipe, confirmation, accept, error in (
    ("REMOTE-010", "remote-password-required", "Continue", False, "Permission denied"),
    ("REMOTE-011", "remote-password-required", "Continue", True, None),
    ("REMOTE-012", "remote-host-untrusted", "Trust Host", False, "Host key verification failed"),
    ("REMOTE-013", "remote-host-untrusted", "Trust Host", True, None),
):
    steps = [
        ("context", "Fixture", "machine"),
        ("click", "Reconnect", "menu"),
        ("dialog", "SSH Authentication"),
    ]
    if accept:
        if confirmation == "Continue":
            steps.append(("type", "fixture-local-only"))
        steps.append(("dialog_button", confirmation))
        if confirmation == "Continue":
            # Separate SSH sessions inspect the server, discover its socket,
            # and establish the live forwarding connection. Each authenticates.
            for _ in range(2):
                steps += [("dialog", "SSH Authentication"),
                          ("type", "fixture-local-only"), ("dialog_button", "Continue")]
        steps += [
            ("finish_dialogs",),
            ("remote_shell",),
            ("remote_workspace_preserved",),
        ]
    else:
        steps += [
            ("dialog_button", "Cancel"),
            ("finish_dialogs",),
            ("absent", "SSH Authentication"),
            ("visible", "Offline"),
            ("remote_rejected", error),
            ("remote_workspace_preserved",),
        ]
    case(
        id,
        ("Accept" if accept else "Cancel") + " real SSH " + confirmation.lower(),
        *steps,
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
            ("field_value", title, ", ".join(value)),
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

case(
    "IME-001",
    "Native preedit stays out of the PTY until its exact Unicode commit",
    ("probe",),
    ("begin_composition",),
    ("probe_no_input",),
    ("finish_composition", True),
    ("probe_hex", "c3a9"),
)
case(
    "IME-002",
    "Cancelling native preedit leaks no input and ordinary typing resumes",
    ("probe",),
    ("begin_composition",),
    ("probe_no_input",),
    ("finish_composition", False),
    ("probe_no_input",),
    ("probe_key", "x"),
    ("probe_hex", "78"),
)

case(
    "IME-003",
    "Real Japanese input keeps preedit out of the PTY and commits exact Hiragana",
    ("probe", "raw", "60"),
    ("input_source", "Japanese"),
    ("click", "Terminal", "terminal", 0),
    ("type", "kana"),
    ("key", "F6"),
    ("probe_no_input",),
    ("preedit_pixels",),
    ("key", "ENTER"),
    ("probe_hex", "e3818be381aa"),
    ("restore_input_source",),
    fixture="native-japanese-input",
)
case(
    "IME-004",
    "Cancelling real Japanese preedit leaks no PTY bytes and ordinary input resumes",
    ("probe", "raw", "60"),
    ("input_source", "Japanese"),
    ("click", "Terminal", "terminal", 0),
    ("type", "kana"),
    ("probe_no_input",),
    ("preedit_pixels",),
    ("key", "ESCAPE"),
    ("probe_no_input",),
    ("restore_input_source",),
    ("click", "Terminal", "terminal", 0),
    ("probe_key", "x"),
    ("probe_hex", "78"),
    fixture="native-japanese-input",
)

for number, split in ((1, False), (2, True)):
    actions = [("resize", 1000, 660)]
    if split:
        actions += [("command", "Split Right"), ("count", "panes", 2)]
    case(
        f"GEOM-{number:03}",
        "Native resize updates PTY geometry and preserves " + ("split panes" if split else "the shell"),
        *actions,
        ("remember_pids",),
        ("remember_layout",),
        ("geometry_probe",),
        ("resize", 900, 580),
        ("geometry_changed",),
        ("same_layout",),
        ("same_pids",),
    )

LONG_TAB_NAME = ("Review 日本語 " + "long workspace notes " * 8).rstrip()
case(
    "LONG-001",
    "A long Unicode tab title stays accessible and survives client relaunch",
    ("resize", 900, 580),
    ("remember_layout",),
    ("remember_pids",),
    ("clipboard", LONG_TAB_NAME),
    ("context", "Tab 1"),
    ("click", "Rename"),
    ("fill", "Tab name", LONG_TAB_NAME),
    ("field_value", "Tab name", LONG_TAB_NAME),
    ("key", "ENTER"),
    ("tab_name", LONG_TAB_NAME),
    ("target_visible", LONG_TAB_NAME, "button"),
    ("run_focused", "printf long-title-focus"),
    ("output", "long-title-focus"),
    ("relaunch",),
    ("target_visible", LONG_TAB_NAME, "button"),
    ("same_layout",),
    ("same_pids",),
    behavior_ids=("rendering",),
)

for i, (source, destination) in enumerate((("Tab 1", "Tab 2"), ("Tab 2", "Tab 1")), 1):
    case(
        f"DRAG-{i:03}",
        f"Drag {source} over {destination} preserves processes",
        ("command", "New Tab"),
        ("key", "ESCAPE"),
        ("count", "tabs", 2),
        ("count", "panes", 2),
        ("remember_tab_order",),
        ("remember_pids",),
        ("drag", source, destination, 0.75 if i == 1 else 0.25),
        ("count", "tabs", 2),
        ("tab_order_reversed",),
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
        ("run_focused", "printf palette-returned-focus"),
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
    (
        "USAGE-003",
        "usage-unknown-account",
        [
            ("click", "Codex", "button"),
            ("visible", "Codex usage"),
            ("visible", "Account identity unavailable. Shown separately for this machine."),
            ("visible", "Local"),
        ],
    ),
    ("USAGE-004", "usage-no-sessions", [("absent", "USAGE")]),
    (
        "USAGE-005",
        "usage-auth-error",
        [
            ("visible", "Sign in again to read usage"),
            ("click", "Codex, fixture@example.invalid", "button"),
            ("visible", "Codex usage"),
            ("visible", "Sign in again to read usage"),
            ("key", "ESCAPE"),
            ("run", "printf usage-auth-shell-alive"),
            ("output", "usage-auth-shell-alive"),
        ],
    ),
    (
        "USAGE-006",
        "usage-rate-limited",
        [
            ("visible", "Usage service rate limited. Retrying later"),
            ("click", "Codex, fixture@example.invalid", "button"),
            ("visible", "Codex usage"),
            ("visible", "Usage service rate limited. Retrying later"),
            ("key", "ESCAPE"),
            ("run", "printf usage-rate-shell-alive"),
            ("output", "usage-rate-shell-alive"),
        ],
    ),
    (
        "REMOTE-006",
        "remote-connected",
        [
            # Provisioning the peer does not complete the client's startup.
            # Prove this UI connection before exercising its Disconnect action.
            ("remote_shell",),
            ("context", "Fixture", "machine"), ("click", "Disconnect", "menu"),
            ("context", "Fixture", "machine"),
            ("disabled", "Disconnect"), ("enabled", "Reconnect", "menu"),
            ("key", "ESCAPE"), ("remote_workspace_preserved",),
        ],
    ),
    (
        "REMOTE-007",
        "remote-connected",
        [
            ("remote_shell",),
            ("context", "Fixture", "machine"),
            ("click", "Edit Remote…", "menu"),
            ("visible", "Save"),
            ("click", "Cancel"),
        ],
    ),
    (
        "REMOTE-008",
        "remote-connected",
        [("context", "Fixture", "machine"), ("click", "Remove Remote", "menu"),
         ("machine_absent", "Fixture"), ("remote_workspace_preserved",)],
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
            ("remote_shell",),
            ("context", "Fixture", "machine"),
            ("click", "Update Server…", "menu"),
            ("visible", "Update When Sessions End"),
        ],
    ),
    (
        "SERVER-002",
        "remote-update-available",
        [
            ("remote_shell",),
            ("context", "Fixture", "machine"),
            ("click", "Update Server…", "menu"),
            ("click", "Update When Sessions End", "button"),
            ("context", "Fixture", "machine"),
            ("visible", "Cancel Scheduled Update"),
        ],
    ),
    (
        "SERVER-003",
        "remote-update-available",
        [
            ("remote_shell",),
            ("context", "Fixture", "machine"),
            ("click", "Update Server…", "menu"),
            ("click", "Update When Sessions End", "button"),
            ("context", "Fixture", "machine"),
            ("click", "Cancel Scheduled Update", "menu"),
            ("context", "Fixture", "machine"),
            ("absent", "Cancel Scheduled Update"),
        ],
    ),
    (
        "SERVER-004",
        "remote-connected",
        [
            ("remote_shell",),
            ("context", "Fixture", "machine"),
            ("click", "Restart Server…", "menu"),
            ("visible", "Restart Server — Fixture"),
            ("visible", "1 running terminal"),
            ("click", "Cancel", "button"),
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
        [("settings", "General"), ("click", "Check for Updates…", "button"),
         ("click", "Details", "disclosure"), ("visible", "signature")],
    ),
    ("UPDATE-008", "update-bad-checksum", [
        ("click", "Upgrade…"), ("click", "Details", "disclosure"), ("visible", "checksum")]),
    ("UPDATE-009", "update-download-failed", [
        ("remember_pids",), ("remember_layout",),
        ("click", "Upgrade…"), ("click", "Details", "disclosure"),
        ("visible", "http status: 503"), ("click", "Cancel"),
        ("installed_version", "0.0.4"), ("same_layout",), ("same_pids",)]),
    (
        "UPDATE-010",
        "update-available",
        [
            ("run", "printf x >> update-executions"),
            ("remember_layout",),
            ("remember_client",),
            ("click", "Upgrade…"),
            ("enabled", "Upgrade and Restart", "button"),
            ("click", "Upgrade and Restart", "button"),
            ("installed_version", "0.0.5"),
            ("client_restarted",),
            ("visible", "Open Release Notes"),
            ("same_layout",),
            ("file_text", "update-executions", "x"),
        ],
    ),
    (
        "UPDATE-011",
        "update-interrupted",
        [
            ("remember_layout",),
            ("remember_pids",),
            ("click", "Recover…"),
            ("click", "Recover", "button"),
            ("absent", "Recover Interrupted Upgrade"),
            ("installed_version", "0.0.4"),
            ("same_layout",),
            ("same_pids",),
        ],
    ),
    (
        "UPDATE-012",
        "update-rollback",
        [
            ("run", "printf x >> rollback-executions"),
            ("remember_layout",),
            ("click", "Upgrade…"),
            ("enabled", "Upgrade and Restart", "button"),
            ("click", "Upgrade and Restart", "button"),
            ("upgrade_rolled_back",),
            ("installed_version", "0.0.4"),
            ("relaunch",),
            ("same_layout",),
            ("file_text", "rollback-executions", "x"),
            ("run", "printf y >> rollback-executions"),
            ("file_text", "rollback-executions", "xy"),
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
            ("fill", "Filter by branch or path", "feature/existing"),
            ("enabled", "Open", "button"),
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
            ("target_visible", "integration.claude.install"),
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
    if id.startswith("RECOVERY-"):
        steps = [("click", "Coordinator", "row"), *steps]
    behaviors = {"USAGE": ("accounts",), "UPDATE": ("update-settings",)}
    mapped = behaviors.get(id.split("-")[0], ())
    if id in {"WT-008", "WT-009"}:
        mapped = ("creation",)
    case(id, recipe.replace("-", " ") + " external user workflow", *steps, fixture=recipe,
         behavior_ids=mapped)

case(
    "REMOTE-014",
    "Enabling persistence changes the remote OS setting without restarting its sessions",
    ("remote_linger", "no"),
    ("click", "Session persistence warning for Fixture", "button"),
    ("enabled", "Enable Persistence", "button"),
    ("click", "Enable Persistence", "button"),
    ("remote_linger", "yes"),
    ("absent", "Session persistence warning for Fixture"),
    ("remote_workspace_preserved",),
    ("remote_shell",),
    fixture="remote-persistence-disabled",
)

case(
    "MULTI-001",
    "Two independent clients share live terminals; closing one preserves the other",
    ("remember_layout",),
    ("remember_pids",),
    ("open_client", "second"),
    ("count", "workspaces", 1),
    ("count", "tabs", 1),
    ("count", "panes", 1),
    ("same_layout",),
    ("same_pids",),
    ("run", "printf second-client-alive; printf x >> shared-client-executions"),
    ("use_client", "primary"),
    ("visible", "second-client-alive"),
    ("close_client", "second"),
    ("run", "printf primary-client-alive; printf y >> shared-client-executions"),
    ("output", "primary-client-alive"),
    ("file_text", "shared-client-executions", "xy"),
    ("same_layout",),
    ("same_pids",),
)
case(
    "MULTI-002",
    "Saving settings in another client invalidates the first client's open draft",
    ("remember_pids",),
    ("settings", "General"),
    ("open_client", "second"),
    ("settings", "General"),
    ("click", "Copy text on selection", "toggle"),
    ("save",),
    ("config", "copy_on_select", True),
    ("use_client", "primary"),
    ("visible", "Settings changed outside this window"),
    ("disabled", "Save"),
    ("click", "Cancel", "button"),
    ("settings", "General"),
    ("save",),
    ("config", "copy_on_select", True),
    ("close_client", "second"),
    ("same_pids",),
)
case(
    "MULTI-003",
    "Tabs created and renamed in separate clients converge without replacing their shells",
    ("open_client", "second"),
    ("command", "New Tab"),
    ("count", "tabs", 2),
    ("remember_layout",),
    ("remember_pids",),
    ("use_client", "primary"),
    ("target_visible", "Tab 2", "button"),
    ("context", "Tab 2", "button"),
    ("click", "Rename", "menu"),
    ("fill", "Tab name", "Shared client tab"),
    ("key", "ENTER"),
    ("tab_name", "Shared client tab"),
    ("use_client", "second"),
    ("target_visible", "Shared client tab", "button"),
    ("same_layout",),
    ("same_pids",),
    ("close_client", "second"),
    ("same_layout",),
    ("same_pids",),
)

case(
    "MULTI-004",
    "The focused client owns shared PTY geometry without inactive window resize churn",
    ("resize", 1000, 660),
    ("remember_layout",),
    ("remember_pids",),
    ("geometry_probe", 120),
    ("geometry_record", "primary"),
    ("open_client", "second"),
    ("resize", 900, 580),
    ("geometry_differs", "primary"),
    ("geometry_record", "second"),
    ("use_client", "primary"),
    ("geometry_matches", "primary"),
    ("use_client", "second"),
    ("geometry_matches", "second"),
    ("close_client", "second"),
    ("geometry_matches", "primary"),
    ("geometry_stop",),
    ("same_layout",),
    ("same_pids",),
    ("run", "printf geometry-owner-restored"),
    ("output", "geometry-owner-restored"),
)

for number, scheduled in ((5, False), (6, True)):
    case(
        f"SERVER-{number:03}",
        ("A scheduled update runs after the last remote shell exits" if scheduled else
         "A forced server update restores layout without replaying shell commands"),
        ("remote_run", "printf x >> /home/fixture/project/server-executions"),
        ("context", "Fixture", "machine"),
        ("enabled", "Update Server…", "menu"),
        ("click", "Update Server…", "menu"),
        ("enabled", "Update When Sessions End" if scheduled else "Update and Restart", "button"),
        ("click", "Update When Sessions End" if scheduled else "Update and Restart", "button"),
        *([("remote_exit",)] if scheduled else []),
        ("remote_server_changed", "update"),
        ("remote_file_text", "server-executions", "x"),
        ("remote_run", "printf y >> /home/fixture/project/server-executions"),
        ("remote_file_text", "server-executions", "xy"),
        fixture="remote-update-available",
    )

case(
    "SERVER-007",
    "Restarting a remote server preserves layout and starts fresh working shells",
    ("remote_run", "printf x >> /home/fixture/project/restart-executions"),
    ("context", "Fixture", "machine"),
    ("enabled", "Restart Server…", "menu"),
    ("click", "Restart Server…", "menu"),
    ("enabled", "Restart Server", "button"),
    ("click", "Restart Server", "button"),
    ("remote_server_changed", "restart"),
    ("remote_file_text", "restart-executions", "x"),
    ("remote_run", "printf y >> /home/fixture/project/restart-executions"),
    ("remote_file_text", "restart-executions", "xy"),
    fixture="remote-connected",
)
case(
    "SERVER-008",
    "Cancelling a scheduled server update prevents replacement after sessions end",
    ("remote_run", "printf x >> /home/fixture/project/cancelled-update-executions"),
    ("context", "Fixture", "machine"),
    ("enabled", "Update Server…", "menu"),
    ("click", "Update Server…", "menu"),
    ("enabled", "Update When Sessions End", "button"),
    ("click", "Update When Sessions End", "button"),
    ("context", "Fixture", "machine"),
    ("click", "Cancel Scheduled Update", "menu"),
    ("remote_exit",),
    ("remote_server_stable", 10),
    ("remote_workspace_preserved",),
    ("remote_file_text", "cancelled-update-executions", "x"),
    fixture="remote-update-available",
)

for index, appearance in enumerate(("Light", "Dark"), 1):
    rendered_text = "NATIVE TERMINAL 7429"
    octal = "".join(f"\\{byte:03o}" for byte in rendered_text.encode())
    case(
        f"VISUAL-{index:03}",
        f"Actual {appearance.lower()} terminal pixels remain legible after reopening the client",
        ("resize", 900, 580),
        ("remember_pids",),
        ("remember_layout",),
        ("settings", "Appearance"),
        ("click", appearance),
        ("save",),
        ("config", "appearance", appearance.lower()),
        ("run", "printf '\\033[2J\\033[H" + octal + "\\n'"),
        ("visible", rendered_text),
        ("visual_text", rendered_text, appearance.lower()),
        ("relaunch",),
        ("visual_text", rendered_text, appearance.lower()),
        ("same_layout",),
        ("same_pids",),
        behavior_ids=("rendering",),
    )

case(
    "VISUAL-003",
    "Resizing preserves actual rendered terminal text and its live shell",
    ("resize", 1000, 660),
    ("remember_layout",),
    ("remember_pids",),
    ("run", "printf '\\033[2J\\033[H" + "".join(f"\\{b:03o}" for b in b"RENDER AFTER RESIZE 5837") + "\\n'"),
    ("visual_text", "RENDER AFTER RESIZE 5837"),
    ("resize", 900, 580),
    ("visual_text", "RENDER AFTER RESIZE 5837"),
    ("same_layout",),
    ("same_pids",),
    behavior_ids=("rendering",),
)

case(
    "UPDATE-013",
    "A new terminal in another client invalidates the prepared upgrade confirmation",
    ("click", "Upgrade…"),
    ("enabled", "Upgrade and Restart", "button"),
    ("open_client", "second"),
    ("command", "New Tab"),
    ("count", "tabs", 2),
    ("remember_layout",),
    ("remember_pids",),
    ("use_client", "primary"),
    ("click", "Upgrade and Restart", "button"),
    ("visible", "Sessions changed after confirmation"),
    ("installed_version", "0.0.4"),
    ("same_layout",),
    ("same_pids",),
    ("close_client", "second"),
    ("same_layout",),
    ("same_pids",),
    fixture="update-available",
)

case(
    "TERM-010",
    "Opening the requested directory does not show a spurious launch error",
    ("count", "workspaces", 1),
    ("run", "pwd; printf launch-directory-ready"),
    ("output", "launch-directory-ready"),
    ("output_repo",),
    ("absent", "Could not open the launch directory"),
)

for index, automatic in enumerate((False, True), 1):
    case(
        f"SELECT-{index:03}",
        f"copy_on_select={automatic} controls physical drag selection and explicit Copy",
        ("resize", 900, 580),
        ("remember_layout",),
        ("remember_pids",),
        ("settings", "General"),
        *([("click", "Copy text on selection", "toggle")] if automatic else []),
        ("save",),
        ("config", "copy_on_select", automatic),
        ("run", "printf '\\033[2J\\033[H" + "".join(f"\\{b:03o}" for b in b"SELECTION") + "\\n'"),
        ("clipboard", "selection clipboard sentinel"),
        ("select_word", "SELECTION"),
        (("clipboard_equals", "SELECTION") if automatic else
         ("clipboard_stable", "selection clipboard sentinel")),
        ("copy",),
        ("clipboard_equals", "SELECTION"),
        ("same_layout",),
        ("same_pids",),
    )

case(
    "WT-010",
    "Cancelling checkout closure preserves files, layout and running terminals",
    ("remember_layout",),
    ("remember_pids",),
    ("context", "Fixture checkout"),
    ("click", "Close Workspace", "menu"),
    ("visible", "Close workspace?"),
    ("click", "Cancel", "button"),
    ("same_layout",),
    ("same_pids",),
    ("file_text", "../Fixture checkout/dirty.txt", "Keep this uncommitted work\n"),
    ("git_branch", "feature/existing"),
    fixture="dirty-checkout",
)
case(
    "WT-011",
    "Closing a checkout preserves its dirty files and branch",
    ("remember_pids",),
    ("context", "Fixture checkout"),
    ("click", "Close Workspace", "menu"),
    ("visible", "Close workspace?"),
    ("click", "Close", "button"),
    ("count", "workspaces", 1),
    ("count", "panes", 1),
    ("remaining_pids",),
    ("file_text", "../Fixture checkout/dirty.txt", "Keep this uncommitted work\n"),
    ("git_branch", "feature/existing"),
    ("run", "pwd"),
    ("output_repo",),
    fixture="dirty-checkout",
)
for number, reenable in ((14, False), (15, True)):
    steps = [
        ("remember_layout",),
        ("remember_pids",),
        ("settings", "General"),
        ("click", "Allow deleting checkout files", "toggle"),
        ("save",),
        ("config", "allow_workspace_deletion", False),
        ("context", "Fixture checkout"),
        ("target_visible", "Close Workspace", "menu"),
        ("absent", "Delete Checkout…"),
        ("key", "ESCAPE"),
        ("relaunch",),
        ("config", "allow_workspace_deletion", False),
        ("context", "Fixture checkout"),
        ("target_visible", "Close Workspace", "menu"),
        ("absent", "Delete Checkout…"),
        ("key", "ESCAPE"),
    ]
    if reenable:
        steps += [
            ("settings", "General"),
            ("click", "Allow deleting checkout files", "toggle"),
            ("save",),
            ("config", "allow_workspace_deletion", True),
            ("context", "Fixture checkout"),
            ("click", "Delete Checkout…", "menu"),
            ("visible", "Delete checkout?"),
            ("click", "Cancel", "button"),
        ]
    case(
        f"WT-{number:03}",
        ("Re-enabling checkout deletion restores its action without deleting on cancellation"
         if reenable else "Disabling checkout deletion removes its action across client relaunch"),
        *steps,
        ("same_layout",),
        ("same_pids",),
        ("file_text", "../Fixture checkout/dirty.txt", "Keep this uncommitted work\n"),
        ("git_branch", "feature/existing"),
        fixture="dirty-checkout",
    )

for number, confirm in ((12, False), (13, True)):
    steps = [
        ("remember_layout",),
        ("remember_pids",),
        ("context", "Fixture checkout"),
        ("click", "Delete Checkout…", "menu"),
        ("visible", "Delete checkout?"),
        ("click", "Delete", "button"),
        ("visible", "Force delete checkout?"),
        ("visible", "Uncommitted and untracked files will be permanently lost."),
    ]
    if confirm:
        steps += [
            ("click", "Force Delete", "button"),
            ("count", "workspaces", 1),
            ("count", "panes", 1),
            ("remaining_pids",),
            ("path_absent", "../Fixture checkout"),
            ("file_text", "tracked.txt", "first\nsecond\n"),
            ("run", "pwd"),
            ("output_repo",),
        ]
    else:
        steps += [
            ("click", "Cancel", "button"),
            ("same_layout",),
            ("same_pids",),
            ("file_text", "../Fixture checkout/dirty.txt", "Keep this uncommitted work\n"),
        ]
    case(
        f"WT-{number:03}",
        "Force deletion removes only the checkout and preserves its branch" if confirm
        else "Cancelling force deletion preserves dirty files, layout and running terminals",
        *steps,
        ("git_branch", "feature/existing"),
        fixture="dirty-checkout",
    )

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

USER_HOOK_SETTINGS = {
    "fixture_user_setting": {"keep": True},
    "hooks": {
        "SessionStart": [
            {
                "matcher": "",
                "hooks": [
                    {
                        "type": "command",
                        "command": "printf fixture-user-hook",
                    }
                ],
            }
        ]
    },
}
for index, (provider, settings_path) in enumerate(
    (("claude", ".claude/settings.json"), ("codex", ".codex/hooks.json")), 9
):
    case(
        f"AGENT-{index:03}",
        f"Installing and removing {provider} integration preserves the user's hooks and settings",
        ("command", "Agent Integrations"),
        ("click", f"integration.{provider}.install"),
        ("target_visible", f"integration.{provider}.remove"),
        ("home_json_contains", settings_path, USER_HOOK_SETTINGS),
        ("click", f"integration.{provider}.remove"),
        ("target_visible", f"integration.{provider}.install"),
        ("home_json_equals", settings_path, USER_HOOK_SETTINGS),
        fixture="provider-custom-hooks",
    )

# Reported identity is exercised by a foreground program, never by injecting
# daemon state or invoking a client callback.
for index, (reported, provider, label) in enumerate(
    (("codex", "codex", "Codex"), ("claude-code", "claude", "Claude")), 1
):
    case(
        f"IDENTITY-{index:03}",
        f"Reported {label} identity survives activity acknowledgement and clear",
        ("program_identity", reported),
        ("pane_field", "agent_provider", provider),
        ("pane_field", "agent_source", "reported"),
        ("pane_field", "agent_role", None),
        ("visible", label),
        ("program_phase", "done"),
        ("pane_field", "status", "done"),
        ("click", "Terminal", "terminal"),
        ("pane_field", "status", "unknown"),
        ("pane_field", "agent_provider", provider),
        ("visible", label),
        ("program_phase", "clear"),
        ("pane_field", "agent_provider", provider),
        ("program_phase", "exit"),
        ("pane_field", "agent_provider", None),
        ("pane_field", "agent_source", None),
        ("run", "printf identity-returned-to-shell"),
        ("output", "identity-returned-to-shell"),
        behavior_ids=("identity",),
    )

case(
    "IDENTITY-003",
    "A terminal title mentioning a provider does not register an agent",
    ("run", "printf '\\033]0;codex build report\\007'"),
    ("pane_field", "agent_provider", None),
    ("pane_field", "agent_source", None),
    ("pane_field", "agent_role", None),
    ("run", "printf title-is-not-an-agent"),
    ("output", "title-is-not-an-agent"),
    behavior_ids=("identity",),
)

case(
    "IDENTITY-004",
    "A client relaunch retains reported identity while a server restart clears it",
    ("remember_layout",),
    ("remember_pids",),
    ("program_identity", "codex"),
    ("pane_field", "agent_provider", "codex"),
    ("pane_field", "agent_source", "reported"),
    ("relaunch",),
    ("same_pids",),
    ("pane_field", "agent_provider", "codex"),
    ("pane_field", "agent_source", "reported"),
    ("restart_server",),
    ("same_layout",),
    ("pane_field", "agent_provider", None),
    ("pane_field", "agent_source", None),
    ("run", "printf restarted-plain-shell"),
    ("output", "restarted-plain-shell"),
    behavior_ids=("identity",),
)

case(
    "IDENTITY-005",
    "A registered coordinator shows its provider and role in the terminal header",
    ("click", "Coordinator", "row"),
    ("visible", "fixture-agent-resumed"),
    ("visible", "Codex · Coordinator"),
    ("agent_field", "provider", "codex"),
    ("registered_pane_fields", {"agent_provider": "codex", "agent_source": "registered",
                               "agent_role": "coordinator"}),
    behavior_ids=("identity",),
    fixture="single-coordinator",
)

for index, (kind, remote) in enumerate(
    (("png", False), ("tiff", False), ("png", True)), 1
):
    entry = "type_shell_command" if kind == "tiff" else "type_terminal"
    case(
        f"IMAGE-{index:03}",
        f"{kind.upper()} paste writes a private image on the {'remote' if remote else 'local'} host",
        *((("remote_shell",),) if remote else (("run", "true"),)),
        ("clipboard_image", kind, True, False),
        (entry, "printf '%s\\n' "),
        ("paste",),
        (entry, "queued-after-image > image-path.txt"),
        ("key", "ENTER"),
        ("image_paste_result", kind, remote),
        fixture="remote-connected" if remote else None,
        behavior_ids=("images",),
    )

case(
    "IMAGE-004",
    "Control-V in a reported agent pane pastes the image before queued input",
    ("program_identity", "codex", "image-arguments"),
    ("pane_field", "agent_source", "reported"),
    ("clipboard_image", "png", True, False),
    ("chord", "ctrl+v"),
    ("type_terminal", "queued-after-image"),
    ("key", "ENTER"),
    ("image_paste_result", "png", False),
    behavior_ids=("images", "identity"),
)

case(
    "IMAGE-005",
    "Oversized clipboard image is rejected without typing into the terminal",
    ("probe",),
    ("clipboard_image", "png", True, True),
    ("paste",),
    ("visible", "Clipboard image is too large"),
    ("probe_no_input",),
    ("probe_key", "x"),
    ("probe_hex", "78"),
    behavior_ids=("images",),
)

case(
    "IMAGE-006",
    "A server without image capability leaves its terminal usable",
    ("remote_shell",),
    ("clipboard_image", "png", True, False),
    ("paste",),
    ("visible", "Image pasting needs an updated Jam server on this machine"),
    ("remote_shell", "printf image-error-shell-usable"),
    ("remote_workspace_preserved",),
    fixture="remote-limited",
    behavior_ids=("images",),
)

case(
    "CREATE-001",
    "Failed worktree creation can be retried once after its base ref is repaired",
    ("command", "New Worktree"),
    ("fill", "Branch name", "creation-retry"),
    ("fill", "Base revision", "repairable-base"),
    ("click", "Create", "button"),
    ("enabled", "Retry", "button"),
    ("enabled", "Dismiss", "button"),
    ("settle_count", "workspaces", 1),
    ("git_no_branch", "creation-retry"),
    ("git_update_ref", "repairable-base"),
    ("click", "Retry", "button"),
    ("created_workspace", "wt/Project ü with spaces/creation-retry", False),
    ("run", "printf new-worktree-ready > creation-result.txt"),
    ("home_file_text", "wt/Project ü with spaces/creation-retry/creation-result.txt", "new-worktree-ready"),
    ("settle_count", "workspaces", 2),
    ("git_branch", "creation-retry"),
    behavior_ids=("creation",),
)

case(
    "CREATE-002",
    "Dismissing a failed worktree restores the previous usable terminal",
    ("remember_layout",),
    ("remember_pids",),
    ("command", "New Worktree"),
    ("fill", "Branch name", "creation-dismiss"),
    ("fill", "Base revision", "invalid-creation-base"),
    ("click", "Create", "button"),
    ("enabled", "Retry", "button"),
    ("click", "Dismiss", "button"),
    ("absent", "Retry"),
    ("same_layout",),
    ("same_pids",),
    ("git_no_branch", "creation-dismiss"),
    ("run", "printf creation-dismissed"),
    ("output", "creation-dismissed"),
    behavior_ids=("creation",),
)

case(
    "CREATE-003",
    "A capability-limited remote creates one workspace in the remote home",
    ("remote_shell",),
    ("command", "New Workspace"),
    ("created_workspace", "/home/fixture", True),
    ("type_terminal", "printf remote-creation-ready > /home/fixture/project/creation-result.txt"),
    ("key", "ENTER"),
    ("remote_file_text", "creation-result.txt", "remote-creation-ready"),
    fixture="remote-limited",
    behavior_ids=("creation", "creation-recovery"),
)

case(
    "TAKEOVER-001",
    "Cancelling a legacy server takeover preserves its running sessions",
    ("visible", "Takeover required", 60),
    ("context", "Fixture", "machine"),
    ("click", "Take Over Server…", "menu"),
    ("visible", "Take over the server on Fixture?"),
    ("visible", "Running terminals will stop"),
    ("enabled", "Take Over and Reconnect", "button"),
    ("click", "Cancel", "button"),
    ("remote_workspace_preserved",),
    ("remote_server_stable", 2),
    fixture="remote-legacy",
    behavior_ids=("takeover",),
)

case(
    "TAKEOVER-002",
    "An explicit legacy takeover restores the saved layout and starts fresh shells",
    ("visible", "Takeover required", 60),
    ("context", "Fixture", "machine"),
    ("click", "Take Over Server…", "menu"),
    ("enabled", "Take Over and Reconnect", "button"),
    ("click", "Take Over and Reconnect", "button"),
    ("visible", "Remote project", 120),
    ("click", "Remote project", "row"),
    ("remote_server_changed", "takeover"),
    ("remote_file_text", "takeover-executions", "x"),
    ("remote_shell", "printf y >> takeover-executions"),
    ("remote_file_text", "takeover-executions", "xy"),
    fixture="remote-legacy",
    behavior_ids=("takeover",),
)

case(
    "DIAG-001",
    "Inspecting an unresponsive local server preserves its daemon and live shell",
    ("remember_layout",),
    ("remember_pids",),
    ("suspend_daemon",),
    ("server_unresponsive",),
    ("context", "Local", "machine"),
    ("absent", "Take Over Server…"),
    ("absent", "Server update required"),
    ("disabled", "Restart Server…", "menu"),
    ("click", "Server Details…", "menu"),
    ("visible", "Server — Local"),
    ("visible", "without stopping running sessions"),
    ("visible", "Local server is not responding"),
    ("enabled", "Retry", "button"),
    ("click", "Retry", "button"),
    ("enabled", "Retry", "button"),
    ("visible", "without stopping running sessions"),
    ("resume_daemon",),
    ("absent", "Server — Local"),
    ("same_layout",),
    ("same_pids",),
    ("run", "printf daemon-recovered-without-restart"),
    ("output", "daemon-recovered-without-restart"),
    behavior_ids=("diagnostics",),
)

case(
    "DIAG-002",
    "A broken SSH path exposes inspection and Retry without enabling destructive actions",
    ("remote_shell",),
    ("peer_transport", True),
    ("visible", "Server unavailable", 60),
    ("context", "Fixture", "machine"),
    ("disabled", "Update Server…", "menu"),
    ("disabled", "Restart Server…", "menu"),
    ("absent", "Take Over Server…"),
    ("click", "Server Details…", "menu"),
    ("visible", "Server — Fixture"),
    ("visible", "without stopping running sessions"),
    ("enabled", "Retry", "button"),
    ("click", "Retry", "button"),
    ("enabled", "Retry", "button"),
    ("remote_workspace_preserved",),
    ("remote_server_stable", 2),
    ("peer_transport", False),
    ("absent", "Server — Fixture"),
    ("remote_shell",),
    ("remote_workspace_preserved",),
    fixture="remote-creation-transport",
    behavior_ids=("diagnostics",),
)

case(
    "UPDATE-SETTINGS-001",
    "Settings prepares and cancels one managed upgrade sheet",
    ("remember_layout",),
    ("remember_pids",),
    ("settings", "General"),
    ("click", "Check for Updates…", "button"),
    ("visible", "Jam 0.0.5 is available"),
    ("enabled", "Upgrade…", "button"),
    ("click", "Upgrade…", "button"),
    ("visible", "Upgrade Jam"),
    ("single_sheet", "Upgrade Jam"),
    ("enabled", "Upgrade and Restart", "button"),
    ("click", "Cancel", "button"),
    ("absent", "Upgrade Jam"),
    ("installed_version", "0.0.4"),
    ("same_layout",),
    ("same_pids",),
    fixture="update-available",
    behavior_ids=("update-settings",),
)

case(
    "ACCOUNT-001",
    "A configured login appears without any active agent session",
    ("visible", "USAGE"),
    ("click", "Codex, fixture@example.invalid", "button"),
    ("visible", "Codex usage"),
    ("visible", "fixture@example.invalid"),
    ("visible", "0 active sessions"),
    ("key", "ESCAPE"),
    ("pane_field", "agent_provider", None),
    ("run", "printf configured-account-shell-usable"),
    ("output", "configured-account-shell-usable"),
    fixture="usage-configured-login",
    behavior_ids=("accounts",),
)

for index, mode in enumerate(("complete", "timeout"), 1):
    case(
        f"RENDER-{index:03}",
        "Synchronized output hides partial redraws" if mode == "complete"
        else "An unterminated synchronized redraw releases its frame after the deadline",
        ("remember_pids",),
        ("synchronized_frames", mode),
        ("run", "printf input-after-synchronized-output"),
        ("output", "input-after-synchronized-output"),
        ("same_pids",),
        behavior_ids=("rendering",),
    )

case(
    "RENDER-003",
    "A focused terminal renders steady, blinking and concealed cursor states",
    ("remember_pids",),
    ("cursor_frames",),
    ("run", "printf input-after-cursor-changes"),
    ("output", "input-after-cursor-changes"),
    ("same_pids",),
    behavior_ids=("rendering",),
)

case(
    "SIDEBAR-001",
    "A narrow inset sidebar keeps the status legible beside a long machine name",
    ("remote_shell",),
    ("resize", 840, 560),
    ("peer_transport", True),
    ("visible", "Server unavailable", 60),
    ("sidebar_status_pixels", "Fixture diagnostics with a name that exceeds the narrow sidebar",
     "Server unavailable"),
    ("peer_transport", False),
    ("remote_shell",),
    ("absent", "Server unavailable"),
    ("remote_workspace_preserved",),
    fixture="remote-long-sidebar",
    behavior_ids=("rendering", "diagnostics"),
)

for index, interrupted in enumerate((False, True), 4):
    case(
        f"CREATE-{index:03}",
        "An interrupted creation resumes its existing operation after Retry" if interrupted
        else "A pending worktree becomes one selected workspace with a usable terminal",
        ("hold_worktree_checkout",),
        ("command", "New Worktree"),
        ("fill", "Branch name", "pending-creation"),
        ("fill", "Base revision", "HEAD"),
        ("click", "Create", "button"),
        ("checkout_pending",),
        ("visible", "Opening workspace"),
        *((("suspend_daemon",), ("visible", "Retry", 35), ("resume_daemon",))
          if interrupted else ()),
        ("release_worktree_checkout",),
        *((("click", "Retry", "button"),) if interrupted else ()),
        ("created_workspace", "wt/Project ü with spaces/pending-creation", False),
        ("absent", "Opening workspace"),
        ("run", "printf pending-worktree-ready > creation-result.txt"),
        ("home_file_text", "wt/Project ü with spaces/pending-creation/creation-result.txt",
         "pending-worktree-ready"),
        ("settle_count", "workspaces", 2),
        behavior_ids=("creation", "creation-recovery") if interrupted else ("creation",),
    )

case(
    "CLOSE-001",
    "A workspace with no live terminals closes without a confirmation",
    ("write_config", "close_empty_tabs = false\nclose_empty_workspaces = false\n"),
    ("relaunch",),
    ("type_terminal", "exit"),
    ("key", "ENTER"),
    ("live_panes", 0),
    ("visible", "Start Shell"),
    ("count", "workspaces", 1),
    ("command", "Close Workspace"),
    ("count", "workspaces", 0),
    ("absent", "Close project and its open worktrees?"),
    ("settle_count", "workspaces", 0),
    ("file_text", "tracked.txt", "first\nsecond\n"),
    behavior_ids=("focus-close",),
)

for index, world in enumerate(("dirty-checkout", "single-coordinator"), 2):
    child = world == "dirty-checkout"
    case(
        f"CLOSE-{index:03}",
        "A live worktree requires confirmation before closing an empty project"
        if child else "A live coordinator requires confirmation before closing its otherwise empty project",
        ("write_config", "close_empty_tabs = false\nclose_empty_workspaces = false\n"),
        ("relaunch",),
        ("click", "Fixture", "row"),
        ("type_terminal", "exit"),
        ("key", "ENTER"),
        ("visible", "Start Shell"),
        ("remember_pids",),
        ("live_panes", 1),
        ("context", "Fixture", "row"),
        ("click", "Close Workspace", "menu"),
        ("visible", "Close project and its open worktrees?"),
        ("click", "Cancel", "button"),
        ("same_pids",),
        ("context", "Fixture", "row"),
        ("click", "Close Workspace", "menu"),
        ("visible", "Close project and its open worktrees?"),
        ("click", "Close", "button"),
        ("count", "workspaces", 0),
        ("settle_count", "workspaces", 0),
        ("settle_count", "panes", 0),
        ("file_text", "tracked.txt", "first\nsecond\n"),
        *((("file_text", "../Fixture checkout/dirty.txt", "Keep this uncommitted work\n"),
           ("git_branch", "feature/existing")) if child else ()),
        fixture=world,
        behavior_ids=("focus-close",),
    )

case(
    "CLOSE-004",
    "A project and worktree with no live terminals close together without confirmation",
    ("write_config", "close_empty_tabs = false\nclose_empty_workspaces = false\n"),
    ("relaunch",),
    ("click", "Fixture checkout", "row"),
    ("type_terminal", "exit"),
    ("key", "ENTER"),
    ("live_panes", 1),
    ("click", "Fixture", "row"),
    ("type_terminal", "exit"),
    ("key", "ENTER"),
    ("live_panes", 0),
    ("count", "workspaces", 2),
    ("context", "Fixture", "row"),
    ("click", "Close Workspace", "menu"),
    ("count", "workspaces", 0),
    ("absent", "Close project and its open worktrees?"),
    ("settle_count", "workspaces", 0),
    ("file_text", "../Fixture checkout/dirty.txt", "Keep this uncommitted work\n"),
    ("git_branch", "feature/existing"),
    fixture="dirty-checkout",
    behavior_ids=("focus-close",),
)

case(
    "IMAGE-007",
    "Control-V is delivered as ordinary terminal input outside an agent pane",
    ("probe",),
    ("clipboard_image", "png", True, False),
    ("chord", "ctrl+v"),
    ("probe_hex", "16"),
    behavior_ids=("images",),
)

case(
    "IMAGE-008",
    "A failed upload keeps prior files and leaves the terminal usable",
    ("block_clipboard_storage",),
    ("probe",),
    ("clipboard_image", "png", True, False),
    ("paste",),
    ("visible", "state/runtime directory must be an owned private directory"),
    ("probe_no_input",),
    ("clipboard_storage_unchanged",),
    ("probe_key", "x"),
    ("probe_hex", "78"),
    behavior_ids=("images",),
)

case(
    "CREATE-006",
    "Opening a local directory twice selects its one existing workspace",
    ("open_directory", "Opened folder ü"),
    ("created_workspace", "Opened folder ü", False),
    ("run", "printf preserved > open-result.txt"),
    ("home_file_text", "Opened folder ü/open-result.txt", "preserved"),
    ("remember_layout",),
    ("remember_pids",),
    ("open_directory", "Opened folder ü"),
    ("same_layout",),
    ("same_pids",),
    ("settle_count", "workspaces", 2),
    behavior_ids=("creation",),
)

case(
    "CREATE-007",
    "Opening a remote directory twice keeps one remote workspace",
    ("remote_shell", "mkdir -p /home/fixture/opened-directory"),
    ("open_workspace",),
    ("visible", "Open Remote Workspace"),
    ("fill", "Path", "/home/fixture/opened-directory"),
    ("click", "Go", "button"),
    ("enabled", "Open", "button"),
    ("click", "Open", "button"),
    ("created_workspace", "/home/fixture/opened-directory", True),
    ("open_workspace",),
    ("fill", "Path", "/home/fixture/opened-directory"),
    ("click", "Go", "button"),
    ("enabled", "Open", "button"),
    ("click", "Open", "button"),
    ("created_workspace", "/home/fixture/opened-directory", True),
    fixture="remote-connected",
    behavior_ids=("creation",),
)

case(
    "TAKEOVER-003",
    "A newer server offers a client upgrade and preserves its live sessions",
    ("visible", "Update Jam required", 60),
    ("context", "Fixture", "machine"),
    ("enabled", "Update Jam…", "menu"),
    ("absent", "Take Over Server…"),
    ("disabled", "Update Server…", "menu"),
    ("key", "ESCAPE"),
    ("remote_workspace_preserved",),
    ("remote_server_stable", 2),
    fixture="update-required",
    behavior_ids=("takeover", "update-settings"),
)

case(
    "FOCUS-001",
    "A background agent's completion is acknowledged only when its tab gains focus",
    ("remember_pane",),
    ("program_identity", "codex", "done-on-file"),
    ("pane_field", "agent_source", "reported"),
    ("command", "New Tab"),
    ("target_visible", "Tab 2", "button"),
    ("release_program_completion",),
    ("remembered_pane_field", "status", "done"),
    ("run", "printf foreground-other-tab"),
    ("output", "foreground-other-tab"),
    ("remembered_pane_field", "status", "done"),
    ("click", "Tab 1", "button"),
    ("remembered_pane_field", "status", "unknown"),
    ("remembered_pane_field", "agent_provider", "codex"),
    ("program_phase", "exit"),
    ("remembered_pane_field", "agent_provider", None),
    behavior_ids=("focus-close", "identity"),
)

case(
    "FOCUS-002",
    "An inactive window keeps completion unread and repeating workspace selection acknowledges it",
    ("remember_pane",),
    ("program_identity", "codex", "done-on-file"),
    ("remembered_pane_field", "agent_source", "reported"),
    ("open_client", "second"),
    ("command", "New Tab"),
    ("target_visible", "Tab 2", "button"),
    ("remember_pids",),
    ("release_program_completion",),
    ("remembered_pane_field", "status", "done"),
    ("run", "printf second-window-keeps-focus"),
    ("output", "second-window-keeps-focus"),
    ("remembered_pane_field", "status", "done"),
    ("use_client", "primary"),
    ("click", "Tab 1", "button"),
    ("remembered_pane_field", "status", "unknown"),
    ("program_phase", "done"),
    ("remembered_pane_field", "status", "done"),
    ("click", "Project ü with spaces", "row"),
    ("remembered_pane_field", "status", "unknown"),
    ("remembered_pane_field", "agent_provider", "codex"),
    ("program_phase", "exit"),
    ("close_client", "second"),
    ("same_pids",),
    behavior_ids=("focus-close", "identity"),
)

case(
    "ACCOUNT-002",
    "The same configured login on two machines shares one account page",
    ("remote_shell",),
    ("usage_requests_from_both_hosts",),
    ("visible", "fixture@example.invalid"),
    ("visible", "Claude"),
    ("absent", "Next usage account"),
    ("click", "Codex, fixture@example.invalid", "button"),
    ("visible", "0 active sessions"),
    ("key", "ESCAPE"),
    ("context", "Fixture", "machine"),
    ("click", "Disconnect", "menu"),
    ("click", "Codex, fixture@example.invalid", "button"),
    ("visible", "0 active sessions"),
    ("absent", "Machine unavailable. Showing last known usage."),
    ("key", "ESCAPE"),
    ("remote_workspace_preserved",),
    fixture="usage-remote-account",
    behavior_ids=("accounts",),
)

case(
    "ACCOUNT-003",
    "Distinct accounts retain separate selectable pages across hosts",
    ("remote_shell",),
    ("usage_requests_from_both_hosts",),
    ("visible", "second@example.invalid"),
    ("click", "Next usage account", "button"),
    ("visible", "fixture@example.invalid"),
    ("visible", "Claude"),
    ("click", "Previous usage account", "button"),
    ("visible", "second@example.invalid"),
    ("click", "Codex, second@example.invalid", "button"),
    ("visible", "0 active sessions"),
    ("key", "ESCAPE"),
    ("context", "Fixture", "machine"),
    ("click", "Disconnect", "menu"),
    ("click", "Codex, second@example.invalid", "button"),
    ("visible", "Machine unavailable. Showing last known usage."),
    ("visible", "25%"),
    ("key", "ESCAPE"),
    ("context", "Fixture", "machine"),
    ("click", "Reconnect", "menu"),
    ("remote_shell",),
    ("click", "Codex, second@example.invalid", "button"),
    ("absent", "Machine unavailable. Showing last known usage."),
    ("key", "ESCAPE"),
    ("remote_workspace_preserved",),
    fixture="usage-remote-other-account",
    behavior_ids=("accounts",),
)

case(
    "ACCOUNT-004",
    "One email shared by different organizations does not combine provider accounts",
    ("remote_shell",),
    ("usage_requests_from_both_hosts",),
    ("visible", "67%"),
    ("usage_page_cycle", ("67%", "25%", "38%")),
    ("remote_workspace_preserved",),
    fixture="usage-remote-other-organization",
    behavior_ids=("accounts",),
)

case(
    "ACCOUNT-005",
    "Disabling a provider hides its usage and leaves the other provider available",
    ("visible", "Claude"),
    ("visible", "Codex"),
    ("settings", "Agents"),
    ("click", "Claude", "toggle"),
    ("save",),
    ("config", "enabled_agents", ["codex"]),
    ("absent", "Claude"),
    ("click", "Codex, fixture@example.invalid", "button"),
    ("visible", "Codex usage"),
    ("key", "ESCAPE"),
    ("run", "printf terminal-available-with-integration-disabled"),
    ("output", "terminal-available-with-integration-disabled"),
    fixture="usage-configured-login",
    behavior_ids=("accounts",),
)

case(
    "ACCOUNT-006",
    "The account page sums active provider processes across local and remote terminals",
    ("run", "bash -c 'exec -a codex /bin/sleep 180' & fixture_job=$!"),
    ("remote_shell", "bash -c 'exec -a codex /bin/sleep 180' & fixture_job=$!"),
    ("usage_requests_from_both_hosts",),
    ("click", "Codex, fixture@example.invalid", "button"),
    ("visible", "2 active sessions"),
    ("key", "ESCAPE"),
    ("absent", "Next usage account"),
    ("remote_workspace_preserved",),
    fixture="usage-remote-account",
    behavior_ids=("accounts",),
)

case(
    "ACCOUNT-007",
    "A rejected remote login shows its own error with no local authorization action",
    ("remote_shell",),
    ("usage_requests_from_both_hosts",),
    ("visible", "Sign in again to read usage"),
    ("click", "Codex, second@example.invalid", "button"),
    ("visible", "Sign in again to read usage"),
    ("absent", "Allow Keychain Access"),
    ("key", "ESCAPE"),
    ("click", "Next usage account", "button"),
    ("click", "Codex, fixture@example.invalid", "button"),
    ("visible", "25%"),
    ("absent", "Sign in again to read usage"),
    ("key", "ESCAPE"),
    ("remote_workspace_preserved",),
    fixture="usage-remote-auth-error",
    behavior_ids=("accounts",),
)

case(
    "TAKEOVER-004",
    "A replacement refused by the remote service rolls back to the previous server",
    ("visible", "Takeover required", 60),
    ("context", "Fixture", "machine"),
    ("click", "Take Over Server…", "menu"),
    ("enabled", "Take Over and Reconnect", "button"),
    ("click", "Take Over and Reconnect", "button"),
    ("takeover_rolled_back",),
    ("visible", "Try Again", 100),
    ("remote_file_text", "takeover-executions", "x"),
    ("enabled", "Try Again", "button"),
    ("click", "Cancel", "button"),
    fixture="remote-takeover-failure",
    behavior_ids=("takeover",),
)

case(
    "TAKEOVER-005",
    "An interrupted remote installation requires explicit recovery before replacing its sessions",
    ("remote_run", "printf x >> recovery-executions"),
    ("interrupt_remote_install",),
    ("context", "Fixture", "machine"),
    ("click", "Update Server…", "menu"),
    ("enabled", "Update and Restart", "button"),
    ("click", "Update and Restart", "button"),
    ("visible", "Could not complete the server action.", 95),
    ("observed_interrupted_install",),
    ("remote_file_text", "recovery-executions", "x"),
    ("click", "Cancel", "button"),
    ("context", "Fixture", "machine"),
    ("click", "Take Over Server…", "menu"),
    ("enabled", "Try Again", "button"),
    ("click", "Try Again", "button"),
    ("visible", "Finish the interrupted server operation and reconnect"),
    ("enabled", "Recover and Reconnect", "button"),
    ("remote_file_text", "recovery-executions", "x"),
    ("click", "Recover and Reconnect", "button"),
    ("remote_server_changed", "restart"),
    ("observed_recovered_install",),
    ("remote_file_text", "recovery-executions", "x"),
    ("remote_run", "printf y >> recovery-executions"),
    ("remote_file_text", "recovery-executions", "xy"),
    fixture="remote-update-available",
    behavior_ids=("takeover",),
)

case(
    "TAKEOVER-006",
    "A takeover confirmation expires when the inspected peer process changes",
    ("visible", "Takeover required", 60),
    ("context", "Fixture", "machine"),
    ("click", "Take Over Server…", "menu"),
    ("enabled", "Take Over and Reconnect", "button"),
    ("restart_peer_outside_client",),
    ("click", "Take Over and Reconnect", "button"),
    ("visible", "The server changed. Inspect it again before taking over", 60),
    ("restarted_peer_preserved",),
    ("click", "Try Again", "button"),
    ("enabled", "Take Over and Reconnect", "button"),
    ("click", "Take Over and Reconnect", "button"),
    ("visible", "Remote project", 120),
    ("click", "Remote project", "row"),
    ("remote_server_changed", "takeover"),
    ("remote_file_text", "takeover-executions", "x"),
    ("remote_shell", "printf y >> takeover-executions"),
    ("remote_file_text", "takeover-executions", "xy"),
    fixture="remote-legacy",
    behavior_ids=("takeover",),
)

case(
    "TAKEOVER-007",
    "Explicit local takeover replaces a foreign daemon and restores the saved workspace",
    ("visible", "Takeover required", 45),
    ("run", "printf x >> local-takeover-executions"),
    ("remember_layout",),
    ("remember_local_server",),
    ("context", "Local", "machine"),
    ("click", "Take Over Server…", "menu"),
    ("visible", "The server belongs to another Jam installation"),
    ("enabled", "Take Over and Reconnect", "button"),
    ("click", "Take Over and Reconnect", "button"),
    ("local_server_replaced",),
    ("same_layout",),
    ("absent", "Takeover required"),
    ("file_text", "local-takeover-executions", "x"),
    ("run", "printf y >> local-takeover-executions"),
    ("file_text", "local-takeover-executions", "xy"),
    fixture="update-foreign-server",
    behavior_ids=("takeover",),
)

case(
    "RENAME-001",
    "The inline tab editor replaces the old rendered label and returns focus",
    ("context", "Tab 1", "button"),
    ("click", "Rename", "menu"),
    ("fill", "Tab name", "BEFORE 9374"),
    ("key", "ENTER"),
    ("tab_name", "BEFORE 9374"),
    ("rendered_label", "BEFORE 9374", None, "BEFORE 9374", "button"),
    ("context", "BEFORE 9374", "button"),
    ("click", "Rename", "menu"),
    ("fill", "Tab name", "AFTER 2846"),
    ("rendered_label", "AFTER 2846", "BEFORE 9374", "Tab name", "field"),
    ("key", "ENTER"),
    ("tab_name", "AFTER 2846"),
    ("run_focused", "printf rename-returned-terminal-focus"),
    ("output", "rename-returned-terminal-focus"),
    behavior_ids=("rendering", "focus-close"),
)

case(
    "CREATE-008",
    "Completing a pending worktree keeps the workspace subsequently selected by the user",
    ("hold_worktree_checkout",),
    ("command", "New Worktree"),
    ("fill", "Branch name", "background-creation"),
    ("fill", "Base revision", "HEAD"),
    ("click", "Create", "button"),
    ("checkout_pending",),
    ("visible", "Opening workspace"),
    ("click", "Project ü with spaces", "row"),
    ("release_worktree_checkout",),
    ("created_workspace", "wt/Project ü with spaces/background-creation", False),
    ("run", "printf original-selection-kept > selection-result.txt"),
    ("file_text", "selection-result.txt", "original-selection-kept"),
    ("settle_count", "workspaces", 2),
    behavior_ids=("creation",),
)

case(
    "CREATE-009",
    "A remote checkout completed through a transport outage is found by explicit Retry",
    ("remote_shell",),
    ("hold_worktree_checkout", "remote"),
    ("command", "New Worktree"),
    ("fill", "Branch name", "transport-creation"),
    ("fill", "Base revision", "HEAD"),
    ("click", "Create", "button"),
    ("peer_checkout", "transport-creation"),
    ("visible", "Opening workspace"),
    ("peer_transport", True),
    ("visible", "Retry", 45),
    ("release_peer_checkout",),
    ("created_workspace", "/home/fixture/wt/project/transport-creation", True),
    ("peer_transport", False),
    ("click", "Retry", "button"),
    ("absent", "Opening workspace"),
    ("type_terminal", "pwd > /home/fixture/project/transport-result.txt"),
    ("key", "ENTER"),
    ("remote_file_text", "transport-result.txt", "/home/fixture/wt/project/transport-creation\n"),
    ("peer_checkout", "transport-creation"),
    fixture="remote-creation-transport",
    behavior_ids=("creation", "creation-recovery"),
)

case(
    "CREATE-010",
    "Local New Workspace opens one ready terminal at the user's home",
    ("command", "New Workspace"),
    ("created_workspace", ".", False),
    ("run", "[ \"$PWD\" = \"$HOME\" ] && printf home-shell-ready > new-workspace.txt"),
    ("home_file_text", "new-workspace.txt", "home-shell-ready"),
    ("settle_count", "workspaces", 2),
    behavior_ids=("creation",),
)

case(
    "CREATE-011",
    "A pending checkout prevents reordering until the new workspace is ready",
    ("open_directory", "Second workspace"),
    ("created_workspace", "Second workspace", False),
    ("click", "Project ü with spaces", "row"),
    ("hold_worktree_checkout",),
    ("command", "New Worktree"),
    ("fill", "Branch name", "reorder-creation"),
    ("fill", "Base revision", "HEAD"),
    ("click", "Create", "button"),
    ("checkout_pending",),
    ("visible", "Opening workspace"),
    ("drag", "Second workspace", "Project ü with spaces", 0.5, 0.12, "row"),
    ("workspace_prefix", "Project ü with spaces", "Second workspace"),
    ("release_worktree_checkout",),
    ("created_workspace", "wt/Project ü with spaces/reorder-creation", False),
    ("drag", "Second workspace", "Project ü with spaces", 0.5, 0.12, "row"),
    ("workspace_prefix", "Second workspace", "Project ü with spaces"),
    ("settle_count", "workspaces", 3),
    ("run", "printf reordered-selection-ready > selected-workspace.txt"),
    ("home_file_text", "Second workspace/selected-workspace.txt", "reordered-selection-ready"),
    behavior_ids=("creation",),
)

for index, completed in enumerate((True, False), 12):
    branch = "restart-creation"
    created_path = "/home/fixture/wt/project/" + branch
    case(
        f"CREATE-{index:03}",
        "Retry finds a persisted checkout after the remote daemon restarts" if completed else
        "Retry refuses to resubmit an unconfirmed checkout after the remote daemon restarts",
        ("remote_shell",),
        ("hold_worktree_checkout", "remote"),
        ("command", "New Worktree"),
        ("fill", "Branch name", branch),
        ("fill", "Base revision", "HEAD"),
        ("click", "Create", "button"),
        ("peer_checkout", branch),
        ("peer_transport", True),
        ("visible", "Retry", 45),
        *((("release_peer_checkout",), ("created_workspace", created_path, True)) if completed else ()),
        ("restart_peer_outside_client",),
        ("peer_transport", False),
        ("click", "Retry", "button"),
        *((
            ("absent", "Retry"),
            ("type_terminal", "pwd > /home/fixture/project/restart-result.txt"),
            ("key", "ENTER"),
            ("remote_file_text", "restart-result.txt", created_path + "\n"),
            ("created_workspace", created_path, True),
        ) if completed else (
            ("visible", "The server restarted. Check the workspace list before starting again."),
            ("click", "Dismiss", "button"),
            ("remote_shell", "printf original-shell-recovered"),
            ("remote_server_changed", "restart"),
        )),
        ("peer_checkout", branch),
        fixture="remote-creation-transport",
        behavior_ids=("creation", "creation-recovery"),
    )

case(
    "CREATE-014",
    "Opening an existing remote worktree starts a shell in its selected branch",
    ("remote_shell", "git worktree add -b peer-existing /home/fixture/remote-existing HEAD"),
    ("command", "Open Worktree"),
    ("fill", "Filter by branch or path", "peer-existing"),
    ("visible", "peer-existing"),
    ("enabled", "Open", "button"),
    ("key", "ENTER"),
    ("created_workspace", "/home/fixture/remote-existing", True),
    ("type_terminal", "{ pwd; git branch --show-current; } > /home/fixture/project/open-branch.txt"),
    ("key", "ENTER"),
    ("remote_file_text", "open-branch.txt", "/home/fixture/remote-existing\npeer-existing\n"),
    fixture="remote-creation-transport",
    behavior_ids=("creation",),
)

case(
    "CREATE-015",
    "A pending worktree protects the previous terminal from input during handoff",
    ("probe", "raw", "180"),
    ("hold_worktree_checkout",),
    ("command", "New Worktree"),
    ("fill", "Branch name", "isolated-handoff"),
    ("fill", "Base revision", "HEAD"),
    ("click", "Create", "button"),
    ("checkout_pending",),
    ("visible", "Opening workspace"),
    ("key", "ESCAPE"),
    ("probe_no_input",),
    ("release_worktree_checkout",),
    ("created_workspace", "wt/Project ü with spaces/isolated-handoff", False),
    ("run", "printf handed-off > creation-focus.txt"),
    ("home_file_text", "wt/Project ü with spaces/isolated-handoff/creation-focus.txt", "handed-off"),
    ("probe_no_input",),
    ("click", "Project ü with spaces", "row"),
    ("probe_key", "x"),
    ("probe_hex", "78"),
    behavior_ids=("creation", "focus-close"),
)

case(
    "UPDATE-SETTINGS-002",
    "A slow Settings download shows one progress sheet and can be cancelled",
    ("remember_layout",),
    ("remember_pids",),
    ("settings", "General"),
    ("click", "Check for Updates…", "button"),
    ("visible", "Jam 0.0.5 is available"),
    ("enabled", "Upgrade…", "button"),
    ("click", "Upgrade…", "button"),
    ("visible", "Downloading and verifying the update…"),
    ("single_sheet", "Upgrade Jam"),
    ("disabled", "Upgrade and Restart", "button"),
    ("click", "Cancel", "button"),
    ("absent", "Upgrade Jam"),
    ("installed_version", "0.0.4"),
    ("same_layout",),
    ("same_pids",),
    fixture="update-slow-prepare",
    behavior_ids=("update-settings",),
)

case(
    "UPDATE-SETTINGS-003",
    "Update preparation routes a foreign live server to a takeover confirmation",
    ("remember_layout",),
    ("remember_pids",),
    ("settings", "General"),
    ("click", "Check for Updates…", "button"),
    ("visible", "Jam 0.0.5 is available"),
    ("enabled", "Upgrade…", "button"),
    ("click", "Upgrade…", "button"),
    ("visible", "Take over the server on Local?", 60),
    ("visible", "The server belongs to another Jam installation"),
    ("enabled", "Take Over and Reconnect", "button"),
    ("click", "Cancel", "button"),
    ("same_layout",),
    ("same_pids",),
    ("installed_version", "0.0.4"),
    fixture="update-foreign-server",
    behavior_ids=("update-settings", "takeover"),
)

case(
    "UPDATE-SETTINGS-004",
    "An unmanaged installation reports a release in Settings without offering an upgrade",
    ("remember_layout",),
    ("remember_pids",),
    ("settings", "General"),
    ("click", "Check for Updates…", "button"),
    ("visible", "Jam 0.0.5 is available"),
    ("disabled", "Upgrade…", "button"),
    ("absent", "Upgrade Jam"),
    ("click", "Cancel", "button"),
    ("absent", "Jam 0.0.5 is available"),
    ("installed_version", "0.0.4"),
    ("same_layout",),
    ("same_pids",),
    fixture="update-unmanaged",
    behavior_ids=("update-settings",),
)

case(
    "UPDATE-SETTINGS-005",
    "An unavailable minimum client release leaves the newer server untouched",
    ("visible", "Update Jam required", 60),
    ("settings", "General"),
    ("enabled", "Check for Updates…", "button"),
    ("click", "Check for Updates…", "button"),
    ("visible", "This machine requires Jam 0.0.5"),
    ("visible", "no matching published update is available"),
    ("absent", "Upgrade…"),
    ("click", "Cancel", "button"),
    ("remote_workspace_preserved",),
    ("remote_server_stable", 2),
    fixture="update-required-without-compatible-release",
    behavior_ids=("update-settings",),
)

case(
    "UPDATE-SETTINGS-006",
    "Settings offers an explicitly requested upgrade after the user skipped its notice",
    ("click", "Skip This Version"),
    ("absent", "Upgrade…"),
    ("settings", "General"),
    ("visible", "Jam 0.0.5 is available"),
    ("enabled", "Upgrade…", "button"),
    ("click", "Upgrade…", "button"),
    ("single_sheet", "Upgrade Jam"),
    ("enabled", "Upgrade and Restart", "button"),
    ("click", "Cancel", "button"),
    ("visible", "Automatically check for updates"),
    ("installed_version", "0.0.4"),
    fixture="update-available",
    behavior_ids=("update-settings",),
)

case(
    "UPDATE-SETTINGS-007",
    "Settings opens one recovery sheet for an actual unfinished app upgrade",
    ("remember_layout",),
    ("remember_pids",),
    ("settings", "General"),
    ("click", "Recover Interrupted Upgrade…", "button"),
    ("single_sheet", "Recover Interrupted Upgrade"),
    ("enabled", "Recover", "button"),
    ("click", "Cancel", "button"),
    ("target_visible", "Recover Interrupted Upgrade…", "button"),
    ("installed_version", "0.0.4"),
    ("same_layout",),
    ("same_pids",),
    fixture="update-interrupted",
    behavior_ids=("update-settings",),
)
