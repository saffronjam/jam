import json
import shlex
import subprocess
import sys
import time
from pathlib import Path

from .driver import wait


def execute(jam, step, memory):
    op, *args = step
    ui, profile = jam.ui, jam.profile
    if op == "run":
        jam.run(*args)
    elif op == "run_focused":
        jam.run_focused(*args)
    elif op == "command":
        jam.command(*args)
    elif op == "settings":
        jam.settings(*args)
    elif op == "save":
        jam.save()
    elif op == "click":
        ui.click(*args)
    elif op == "context":
        ui.context(*args)
    elif op == "fill":
        ui.fill(*args)
    elif op == "type":
        ui.type(*args)
    elif op == "chord":
        ui.chord(*args)
    elif op == "key":
        ui.key(*args)
    elif op == "visible":
        ui.expect(*args)
    elif op == "target_visible":
        ui.element(*args)
    elif op == "absent":
        wait(lambda: not ui.has(args[0]), f"absence of {args[0]}")
        until = time.monotonic() + 2
        while time.monotonic() < until:
            assert not ui.has(args[0]), f"Unexpected visible {args[0]}"
            time.sleep(0.1)
    elif op == "disabled":
        assert not ui.element(args[0]).is_enabled(), f"{args[0]} must be disabled"
    elif op == "count":
        jam.count(*args)
    elif op == "remote_workspace_preserved":
        ids = json.loads((profile.root / "fixture-ids.json").read_text())
        state = profile.call("--remote", ids["remote"], "workspace", "list")["State"]
        assert [workspace["id"] for workspace in state["workspaces"]] == [ids["remote_workspace"]]
        info = profile.call("--remote", ids["remote"], "daemon", "info")["HostInfo"]
        assert (info["build"], info["incarnation"]) == (
            ids["remote_build"], ids["remote_incarnation"]
        )
        assert {pane["id"]: pane["session_generation"] for pane in state["panes"]} == ids["remote_sessions"]
    elif op in ("remember_layout", "same_layout"):

        def identities():
            state = profile.layout()
            return {
                "workspaces": [
                    (workspace["id"], [tab["id"] for tab in workspace["tabs"]])
                    for workspace in state["workspaces"]
                ],
                "panes": sorted(pane["id"] for pane in state["panes"]),
            }

        if op == "remember_layout":
            memory["layout"] = identities()
            assert memory["layout"]["panes"], "No starting terminal to preserve"
        else:
            wait(
                lambda: identities() == memory["layout"],
                "preserved workspace, tab and pane identities",
            )
    elif op == "installed_version":

        def version():
            value = subprocess.run(
                [str(profile.cli), "--version"],
                check=True,
                capture_output=True,
                text=True,
                env=profile.env,
                timeout=10,
            ).stdout.strip()
            return value == "jam " + args[0]

        wait(version, "installed release " + args[0], timeout=90)
    elif op == "settle_count":
        # Negative assertion remains true through a bounded observation window.
        until = time.monotonic() + 2
        while time.monotonic() < until:
            state = profile.layout()
            objects = (
                state[args[0]]
                if args[0] != "tabs"
                else [t for w in state["workspaces"] for t in w["tabs"]]
            )
            assert len(objects) == args[1], f"Unexpected {args[0]} count during negative assertion"
            time.sleep(0.1)
    elif op == "write_config":
        profile.write_config(*args)
    elif op == "config":
        jam.expect_config(*args)
    elif op == "config_default":
        if profile.config.exists():
            jam.expect_config(*args)
        else:
            # Save a freshly opened draft to observe actual effective defaults.
            jam.settings()
            jam.save()
            jam.expect_config(*args)
    elif op == "relaunch":
        ui.quit()
        ui.launch()
        wait(lambda: ui.has("Terminal"), "terminal after reopen")
    elif op == "begin_input":
        jam.terminal()
        ui.type(args[0])
        ui.key("ENTER")
    elif op == "clipboard":
        ui.clipboard(*args)
    elif op == "paste":
        ui.chord("cmd+v" if ui.platform == "macos" else "ctrl+shift+v")
    elif op == "middle":
        ui.middle(*args)
    elif op == "drag":
        ui.drag(*args)
    elif op == "field_value":
        wait(lambda: ui.field_value(args[0]) == args[1], "field value")
    elif op == "file_exists":
        assert (profile.repo / args[0]).is_file()
    elif op == "agent_field":

        def matches():
            agents = profile.call("coordinate", "list")["Coordination"]["agents"]
            return len(agents) == 1 and agents[0].get(args[0]) == args[1]

        wait(matches, f"single agent {args[0]}={args[1]}")
    elif op in ("remember_agent_session", "same_agent_session"):
        agents = profile.call("coordinate", "list")["Coordination"]["agents"]
        assert len(agents) == 1 and agents[0]["session"]
        current = (agents[0]["id"], agents[0]["session"])
        if op == "remember_agent_session":
            memory["agent_session"] = current
        else:
            assert current == memory["agent_session"], "Resume replaced the saved conversation"
    elif op in ("remember_provider_inputs", "provider_inputs_unchanged"):

        def inputs():
            path = profile.home / "provider-events.jsonl"
            return [line for line in path.read_text().splitlines() if "input" in json.loads(line)]

        if op == "remember_provider_inputs":
            memory["provider_inputs"] = inputs()
        else:
            until = time.monotonic() + 2
            while time.monotonic() < until:
                assert inputs() == memory["provider_inputs"], (
                    "Resume submitted unintended provider input"
                )
                time.sleep(0.1)
    elif op == "probe":
        output = profile.repo / "probe.json"
        probe = Path(__file__).with_name("terminal_probe.py")
        command = shlex.join([sys.executable, str(probe), str(output), *args])
        jam.terminal()
        ui.type(command)
        ui.key("ENTER")
        wait(lambda: output.with_suffix(".ready").exists(), "raw PTY probe ready")
    elif op == "probe_key":
        if "+" in args[0]:
            ui.chord(args[0])
        elif args[0].isupper():
            ui.key(args[0])
        else:
            ui.type(args[0])
    elif op == "probe_hex":
        output = profile.repo / "probe.json"
        wait(lambda: output.exists(), "raw PTY probe result")
        assert json.loads(output.read_text())["hex"] == args[0]
    elif op in ("probe_key_events", "probe_mouse_events"):
        import re

        output = profile.repo / "probe.json"
        wait(output.exists, "terminal event probe result")
        data = bytes.fromhex(json.loads(output.read_text())["hex"])
        if op == "probe_key_events":
            assert re.fullmatch(rb"\x1b\[120(?:;1(?::1)?)?u\x1b\[120;1:3u", data), data
        else:
            events = re.findall(rb"\x1b\[<0;(\d+);(\d+)([Mm])", data)
            assert len(events) == 2 and events[0][2] == b"M" and events[1][2] == b"m", data
            assert events[0][:2] == events[1][:2] and all(int(v) > 0 for v in events[0][:2]), data
    elif op in ("file_text", "home_file_text"):
        path = (profile.repo if op == "file_text" else profile.home) / args[0]
        wait(lambda: path.exists() and path.read_text() == args[1], f"file {args[0]} content")
    elif op in ("home_json_contains", "home_json_equals"):
        path = profile.home / args[0]

        def contains(actual, expected):
            if isinstance(expected, dict):
                return isinstance(actual, dict) and all(
                    key in actual and contains(actual[key], value)
                    for key, value in expected.items()
                )
            if isinstance(expected, list):
                return isinstance(actual, list) and all(
                    any(contains(value, item) for value in actual) for item in expected
                )
            return type(actual) is type(expected) and actual == expected

        def matches():
            if not path.exists():
                return False
            value = json.loads(path.read_text())
            return (
                json.dumps(value, sort_keys=True) == json.dumps(args[1], sort_keys=True)
                if op == "home_json_equals"
                else contains(value, args[1])
            )

        wait(matches, f"preserved user JSON {args[0]}")
    elif op in {"output", "output_repo", "output_regex", "no_output"}:
        import re

        expected = str(profile.repo) if op == "output_repo" else args[0]

        def matches():
            text = "\n".join(profile.capture(p["id"]) for p in profile.layout()["panes"])
            if op == "output_regex":
                return re.search(expected, text)
            return expected not in text if op == "no_output" else expected in text

        wait(matches, f"terminal {op}: {expected}")
        if op == "no_output":
            until = time.monotonic() + 2
            while time.monotonic() < until:
                assert matches(), f"Unexpected terminal output: {expected}"
                time.sleep(0.1)
    elif op == "remember_pids":
        memory["pids"] = jam.pids()
    elif op in ("remember_tab_order", "tab_order_reversed"):

        def order():
            return [
                tab["id"]
                for workspace in profile.layout()["workspaces"]
                for tab in workspace["tabs"]
            ]

        if op == "remember_tab_order":
            memory["tab_order"] = order()
        else:
            wait(lambda: order() == list(reversed(memory["tab_order"])), "reordered tabs")
    elif op == "same_pids":
        assert jam.pids() == memory["pids"], "Shell process changed across client operation"
    elif op == "restart_server":
        ui.context("Local")
        ui.click("Restart Server…")
        ui.click("Restart Server", "button")
        wait(lambda: ui.has("Terminal"), "fresh terminal after server restart")
    elif op == "git_branch":
        wait(
            lambda: args[0] in profile.git("branch", "--format=%(refname:short)").splitlines(),
            "created branch",
        )
    elif op == "git_no_branch":
        assert args[0] not in profile.git("branch", "--format=%(refname:short)").splitlines()
    elif op == "config_symlink":
        target = profile.root / "settings-target.toml"
        target.write_text('appearance = "system"\n')
        profile.config.unlink(missing_ok=True)
        profile.config.symlink_to(target)
    elif op == "symlink_preserved":
        assert profile.config.is_symlink()
    else:
        raise ValueError(f"Unknown scenario operation {op!r}")
