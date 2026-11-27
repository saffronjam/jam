import json
import os
import signal
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
    elif op == "suspend_daemon":
        memory["suspended_pid"] = profile.suspend_daemon()
        wait(lambda: (profile.owned_daemon_status(memory["suspended_pid"]) or "").startswith("T"),
             "the owned daemon suspended", timeout=5)
    elif op == "resume_daemon":
        profile.resume_daemon()
        pid = memory["suspended_pid"]
        wait(lambda: (status := profile.owned_daemon_status(pid)) and not status.startswith("T"),
             "the same owned daemon resumed", timeout=5)
    elif op == "git_update_ref":
        profile.git("update-ref", "refs/tags/" + args[0], "HEAD")
    elif op == "hold_worktree_checkout":
        remote = bool(args and args[0] == "remote")
        root = Path("/home/fixture") if remote else profile.root
        ready = shlex.quote(str(root / "checkout.ready"))
        release = shlex.quote(str(root / "checkout.release"))
        script = f"""#!/bin/sh
set -eu
git branch --show-current >> {ready}
remaining=900
while [ ! -e {release} ]; do
  test "$remaining" -gt 0
  remaining=$((remaining - 1))
  sleep 0.1
done
"""
        if remote:
            from .remote import peer_command

            peer_command(profile, "umask 077; cd /home/fixture/project; "
                         f"printf '%s' {shlex.quote(script)} > .git/hooks/post-checkout; "
                         "chmod 700 .git/hooks/post-checkout")
        else:
            hook = profile.repo / ".git/hooks/post-checkout"
            hook.write_text(script)
            hook.chmod(0o700)
    elif op == "checkout_pending":
        wait((profile.root / "checkout.ready").exists, "real Git post-checkout hook", timeout=30)
    elif op == "release_worktree_checkout":
        profile.release_checkout()
    elif op == "peer_checkout":
        from .remote import peer_command

        branch, = args
        wait(lambda: peer_command(profile, "if test -s ~/checkout.ready; then cat ~/checkout.ready; fi")
             == branch + "\n", "one remote Git checkout of " + branch, timeout=45)
    elif op == "release_peer_checkout":
        from .remote import peer_command

        peer_command(profile, "touch ~/checkout.release")
    elif op == "peer_transport":
        cut = profile.root / "ssh.cut"
        receipt = profile.root / "ssh.blocked"
        assert (profile.root / "ssh-transport.ready").is_file()
        if args[0]:
            cut.touch(exist_ok=False)
        else:
            cut.unlink()
        wait(lambda: receipt.exists() == args[0], "owned SSH transport switched", timeout=5)
    elif op == "created_workspace":
        path, remote = args
        if not remote:
            path = str((profile.home / path).resolve())
        if remote:
            from .remote import peer_call

            observe = lambda: peer_call(profile, "workspace", "list")["State"]
        else:
            observe = profile.layout

        def ready():
            state = observe()
            matches = [entry for entry in state["workspaces"]
                       if (entry["path"] if remote else str(Path(entry["path"]).resolve())) == path]
            if not matches:
                return False
            assert len(matches) == 1, "Creation produced duplicate workspace paths"
            entry = matches[0]
            if len(entry["tabs"]) != 1:
                return False
            pane = entry.get("active_pane")
            return any(p["id"] == pane and not p["exited"] and p.get("session_generation")
                       for p in state["panes"])

        wait(ready, f"one live initial terminal at {path}", timeout=60)
    elif op in {"open_client", "use_client", "close_client"}:
        if jam.clients is None:
            from .clients import Clients

            jam.clients = Clients(ui)
        if op == "open_client":
            jam.ui = jam.clients.open(args[0])
        elif op == "use_client":
            jam.ui = jam.clients.select(args[0])
        else:
            jam.ui = jam.clients.close(args[0])
    elif op in {"remote_shell", "remote_run"}:
        jam.remote_shell(*args)
    elif op == "remote_exit":
        ui.click("Remote project", "row")
        jam.terminal()
        ui.type("exit")
        ui.key("ENTER")
    elif op == "open_file_preview":
        from .previews import open_file

        open_file(jam, *args)
    elif op == "preview_playback":
        from .previews import set_playback

        set_playback(ui, *args)
    elif op == "preview_pixels":
        from .previews import preview_pixels

        preview_pixels(jam, *args)
    elif op in {"remember_remote_tabs", "same_remote_tabs"}:
        from .remote import peer_call

        def live_tabs():
            state = peer_call(profile, "workspace", "list")["State"]
            [workspace] = state["workspaces"]
            tabs = [t["id"] for t in workspace["tabs"]]
            sessions = {p["id"]: p.get("session_generation") for p in state["panes"] if not p["exited"]}
            return (tabs, sessions) if len(tabs) == len(sessions) == 2 and all(sessions.values()) else None

        snapshot = wait(live_tabs, "two committed remote tabs with distinct live terminals")
        if op == "remember_remote_tabs":
            memory["remote_tabs"] = snapshot
        else:
            assert memory["remote_tabs"] == snapshot
    elif op == "remote_server_changed":
        from .remote import server_changed

        server_changed(profile, args[0])
    elif op == "takeover_rolled_back":
        from .remote import takeover_rolled_back

        takeover_rolled_back(profile)
    elif op == "interrupt_remote_install":
        from .remote import interrupt_remote_install

        interrupt_remote_install(profile)
    elif op == "restart_peer_outside_client":
        from .remote import restart_peer_outside_client, server_transactions

        memory["restarted_peer"] = restart_peer_outside_client(profile)
        memory["peer_transactions"] = server_transactions(profile)
    elif op == "restarted_peer_preserved":
        from .remote import peer_call, server_transactions

        after = peer_call(profile, "daemon", "info")["HostInfo"]
        before = memory["restarted_peer"]
        assert (after["build"], after["incarnation"]) == (before["build"], before["incarnation"])
        transactions = server_transactions(profile)
        assert sorted(transactions, key=lambda tx: tx["id"]) == sorted(
            memory["peer_transactions"], key=lambda tx: tx["id"])
    elif op == "observed_interrupted_install":
        from .remote import interrupted_install_receipt

        memory["interrupted_install"] = interrupted_install_receipt(profile)
    elif op == "observed_recovered_install":
        from .remote import server_transactions

        before = memory["interrupted_install"]
        [after] = [receipt for receipt in server_transactions(profile) if receipt["id"] == before["id"]]
        assert after == {**before, "phase": "rolled_back"}
        (profile.root / "fixture-server-recovery.json").write_text(json.dumps({
            "before": before, "after": after,
        }, indent=2))
    elif op == "remote_file_text":
        from .remote import peer_command

        name, expected = args
        path = shlex.quote("/home/fixture/project/" + name)
        wait(lambda: peer_command(profile, f"if test -f {path}; then printf ready:; cat -- {path}; fi")
             == "ready:" + expected, f"exact remote file contents for {name}")
    elif op == "remote_server_stable":
        from .remote import peer_call

        ids = json.loads((profile.root / "fixture-ids.json").read_text())
        until = time.monotonic() + args[0]
        while True:
            info = peer_call(profile, "daemon", "info")["HostInfo"]
            assert (info["build"], info["incarnation"]) == (ids["remote_build"], ids["remote_incarnation"])
            if time.monotonic() >= until:
                break
            time.sleep(0.5)
    elif op == "remote_linger":
        from .remote import peer_command

        wait(lambda: peer_command(profile, "loginctl show-user fixture -p Linger --value").strip()
             == args[0], "remote operating system Linger=" + args[0])
    elif op == "machine_absent":
        wait(lambda: not ui.machine_elements(args[0]), f"removed machine {args[0]}")
    elif op == "remote_rejected":
        ids = json.loads((profile.root / "fixture-ids.json").read_text())
        try:
            profile.call("--remote", ids["remote"], "workspace", "list")
        except subprocess.CalledProcessError as error:
            assert args[0].lower() in error.stderr.lower(), error.stderr
        else:
            raise AssertionError("The rejected remote unexpectedly accepted a connection")
    elif op == "run_focused":
        jam.run_focused(*args)
    elif op == "command":
        jam.command(*args)
    elif op == "settings":
        jam.settings(*args)
    elif op == "save":
        jam.save()
    elif op == "click_workspace_row":
        ui.click_workspace_row(*args)
    elif op == "click":
        ui.click(*args)
    elif op == "context":
        ui.context(*args)
    elif op == "fill":
        ui.fill(*args)
    elif op == "query_burst":
        ui.query_burst(*args)
    elif op == "type":
        ui.type(*args)
    elif op == "chord":
        ui.chord(*args)
    elif op == "key":
        ui.key(*args)
    elif op == "input_source":
        ui.input_source(*args)
    elif op == "restore_input_source":
        ui.restore_input_source()
    elif op == "visible":
        ui.expect(*args)
    elif op == "server_unresponsive":
        def unresponsive():
            texts = ui.texts()
            return "Not responding" in texts or "Server unavailable" in texts

        wait(unresponsive, "unresponsive owned server status", timeout=60)
    elif op == "single_sheet":
        ui.expect_single_sheet(*args)
    elif op == "stable_visible":
        text, seconds, *scope = args
        visible = ui.has if scope == ["any"] else ui.has_exact_label
        wait(lambda: visible(text), f"visible label {text!r}")
        until = time.monotonic() + seconds
        while time.monotonic() < until:
            assert visible(text), f"A later result replaced visible {text}"
            time.sleep(.1)
    elif op == "dialog":
        ui.dialog(*args)
    elif op == "dialog_button":
        ui.dialog_button(*args)
    elif op == "finish_dialogs":
        ui.finish_dialogs()
    elif op == "visual_text":
        from .visual import terminal_text

        terminal_text(ui, *args)
    elif op == "theme_pixels":
        from .visual import theme_pixels

        theme_pixels(jam, memory, *args)
    elif op == "remember_terminal_width":
        memory["terminal_width"] = ui.element("Terminal", "terminal", index=0).rect["width"]
    elif op == "narrower_terminal":
        wait(lambda: ui.element("Terminal", "terminal", index=0).rect["width"]
             < memory["terminal_width"] * 0.65, "split terminal shrinks before daemon reply", timeout=5)
    elif op == "cursor_frames":
        from .visual import cursor_frames

        cursor_frames(jam)
    elif op == "control_value":
        name, expected = args
        role = "progress" if expected.endswith(" percent") else "button"

        def matches_value():
            target = ui._find_element(name, role, None)
            if target is None:
                return False
            value = target.get_attribute("value" if ui.platform == "macos" else "description")
            if value == expected:
                return True
            if ui.platform == "macos" and role == "progress":
                # AppKit may expose the indicator's numeric AXValue instead of its spoken value.
                number = expected.removesuffix(" percent")
                return value in {number + "%", str(int(number) / 100)}
            return False
        wait(matches_value, f"{name} shows {expected}")
    elif op == "no_control":
        name, *roles = args
        wait(lambda: ui._find_element(name, roles[0] if roles else None, None) is None,
             f"{name} leaves the desktop")
    elif op == "preview_integrations":
        from .previews import check_integrations

        check_integrations(ui, args[0])
    elif op == "target_visible":
        ui.element(*args)
    elif op == "absent":
        text, *deadline = args
        wait(lambda: not ui.has(text), f"absence of {text}", *deadline)
        until = time.monotonic() + 2
        while time.monotonic() < until:
            assert not ui.has(text), f"Unexpected visible {text}"
            time.sleep(0.1)
    elif op == "disabled":
        assert not ui.element(*args).is_enabled(), f"{args[0]} must be disabled"
    elif op == "enabled":
        name, role, *deadline = args
        ui.element(name, role, enabled=True, timeout=deadline[0] if deadline else 20)
    elif op == "count":
        jam.count(*args)
    elif op == "live_panes":
        wait(lambda: sum(not pane["exited"] for pane in profile.layout()["panes"]) == args[0],
             f"exactly {args[0]} live terminal(s)")
    elif op == "workspace_prefix":
        expected = [(profile.home / path).resolve() for path in args]
        wait(lambda: [Path(w["path"]).resolve() for w in profile.layout()["workspaces"]][:len(expected)] == expected,
             "the reordered workspace paths " + repr(args))
    elif op == "tab_name":
        wait(lambda: any(tab["name"] == args[0] for workspace in profile.layout()["workspaces"]
                         for tab in workspace["tabs"]), f"committed tab name {args[0]!r}")
    elif op in {"remember_remote_workspace", "remote_workspace_preserved"}:
        from .remote import peer_call

        ids = json.loads((profile.root / "fixture-ids.json").read_text())
        state = peer_call(profile, "workspace", "list")["State"]
        assert [workspace["id"] for workspace in state["workspaces"]] == [ids["remote_workspace"]]
        info = peer_call(profile, "daemon", "info")["HostInfo"]
        sessions = {pane["id"]: pane["session_generation"] for pane in state["panes"]}
        assert info["build"] == ids["remote_build"]
        if op == "remember_remote_workspace":
            assert sessions and sessions.keys() == ids["remote_sessions"].keys()
            memory["remote_workspace_before"] = info["incarnation"], sessions
        else:
            expected = memory.get("remote_workspace_before",
                                  (ids["remote_incarnation"], ids["remote_sessions"]))
            assert (info["incarnation"], sessions) == expected
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
    elif op == "remember_client":
        memory["client_pids"] = ui.application_pids()
        assert len(memory["client_pids"]) == 1, "Expected one private application process"
    elif op == "client_restarted":
        ui.follow_restart(memory["client_pids"])
    elif op == "upgrade_rolled_back":
        def rolled_back():
            journals = list((profile.state / "upgrades").glob("apply-*/journal.json"))
            if len(journals) != 1:
                return False
            value = json.loads(journals[0].read_text())
            if value["phase"] != "rolled_back" or "healthy" not in (value["error"] or ""):
                return False
            assert value["staged"]["release"]["version"] == "0.0.5"
            log = journals[0].parent / "daemon.log"
            assert "E2E candidate daemon startup failure" in log.read_text()
            return True

        wait(rolled_back, "genuine candidate execution and automatic rollback", timeout=90)
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
        ui.element("Terminal", "terminal", index=0)
    elif op == "begin_input":
        jam.terminal()
        ui.type(args[0])
        ui.key("ENTER")
    elif op == "clipboard":
        ui.clipboard(*args)
    elif op == "clipboard_image":
        from .clipboard import image_fixture

        kind, mixed, oversized = args
        image, _ = image_fixture(kind)
        if oversized:
            image = image.ljust(16 * 1024 * 1024 + 1, b"\0")
        ui.clipboard_image(kind, image, "unexpected text flavor" if mixed else None)
    elif op == "block_clipboard_storage":
        marker = profile.temporary / f"jam-clipboard-{os.geteuid()}"
        with marker.open("x") as file:
            file.write("Keep this existing file")
    elif op == "clipboard_storage_unchanged":
        marker = profile.temporary / f"jam-clipboard-{os.geteuid()}"
        assert marker.read_text() == "Keep this existing file"
        assert list(profile.temporary.glob("jam-clipboard-*/*.png")) == []
    elif op == "open_workspace":
        jam.open_workspace()
    elif op == "open_directory":
        path = profile.home / args[0]
        path.mkdir(exist_ok=True)
        jam.open_workspace()
        ui.choose_directory(path)
    elif op == "usage_requests_from_both_hosts":
        events = profile.root / "https.events"

        def received():
            if not events.exists():
                return False
            lines = events.read_text().splitlines(keepends=True)
            requests = [json.loads(line) for line in lines if line.endswith("\n")]
            hosts = {entry["fixture_host"] for entry in requests
                     if entry["path"] == "/codex/usage" and entry["authenticated"]}
            return hosts == {"local", "remote"}

        wait(received, "real usage requests from both fixture hosts", timeout=60)
    elif op in {"remember_usage_requests", "usage_requests_unchanged"}:
        events = profile.root / "https.events"

        def readings():
            if not events.exists():
                return []
            return [line for line in events.read_text().splitlines(keepends=True)
                    if line.endswith("\n") and json.loads(line)["path"] == "/codex/usage"]

        if op == "remember_usage_requests":
            memory["usage_requests"] = wait(readings, "an actual Codex usage service request")
        else:
            deadline = time.monotonic() + 3
            while time.monotonic() < deadline:
                assert readings() == memory["usage_requests"], "Usage reached the server during backoff"
                time.sleep(0.1)
    elif op == "reset_offer":
        from .reset_offers import offer

        offer(profile, *args)
    elif op == "reset_receipts":
        from .reset_offers import receipts

        receipts(profile, *args)
    elif op in {"remember_reset_reads", "same_reset_reads"}:
        from .reset_offers import reads

        if op == "remember_reset_reads":
            memory["reset_reads"] = reads(profile)
            assert memory["reset_reads"] > 0
        else:
            assert reads(profile) == memory["reset_reads"]
    elif op in {"remember_release_downloads", "same_release_downloads"}:
        events = (profile.root / "https.events").read_text().splitlines()
        urls = [entry["path"] for entry in map(json.loads, events)
                if entry["path"].startswith("/releases/download/")
                and entry["path"].endswith((".zip", ".tar.gz"))]
        if op == "remember_release_downloads":
            assert urls
            memory["release_downloads"] = urls
        else:
            assert urls == memory["release_downloads"]
    elif op == "usage_service_error":
        scenario = profile.root / "service-world.json"
        state = json.loads(scenario.read_text())
        state["http_error"] = args[0]
        update = scenario.with_suffix(".new")
        update.write_text(json.dumps(state))
        update.replace(scenario)
    elif op == "usage_page_cycle":
        (values,) = args

        def selected():
            text = ui.texts()
            visible = [value for value in values if value in text]
            return visible[0] if len(visible) == 1 else None

        first = wait(selected, "one account's usage on a page")
        visited = {first}
        previous = first
        for _ in values:
            ui.click("Next usage account", "button")
            current = wait(lambda: value if (value := selected()) != previous else None,
                           "the next distinctly measured usage page")
            if len(visited) < len(values):
                assert current not in visited, "Account pagination wrapped before visiting every page"
            visited.add(current)
            previous = current
        assert previous == first and visited == set(values)
    elif op == "type_terminal":
        ui.type(args[0])
    elif op == "type_shell_command":
        ui.type_shell_command(args[0])
    elif op == "image_paste_result":
        from .clipboard import verify_image

        verify_image(profile, *args)
    elif op == "synchronized_frames":
        from .visual import synchronized_frames

        synchronized_frames(jam, *args)
    elif op == "rendered_label":
        from .dialogs import screen_target

        label, forbidden, name, role = args
        prefix = f"visual-label-{len(list(profile.repo.glob('visual-label-*-target.json'))):03}"
        screen_target(ui, label, prefix=prefix, forbidden=forbidden,
                      region=lambda: ui.element(name, role).rect, mode="7")
    elif op == "sidebar_status_pixels":
        from .dialogs import screen_target

        name, status = args
        window = ui.window_rect()
        evidence = {"window": window}
        for role in ("row", "machine"):
            row = ui.element(name, role).rect
            assert row["x"] >= window["x"] + 4, f"{role} reaches the outer window edge"
            assert row["y"] + row["height"] <= window["y"] + window["height"]
            box = screen_target(ui, status, prefix=f"visual-sidebar-{role}", bounds=True,
                                region=lambda: ui.element(name, role).rect)
            assert box["x"] + box["width"] <= row["x"] + row["width"] - 2
            evidence[role] = {"bounds": row, "status": box}
        (profile.repo / "visual-sidebar-geometry.json").write_text(json.dumps(evidence, indent=2))
    elif op in {"clipboard_equals", "clipboard_stable"}:
        wait(lambda: ui.clipboard() == args[0], "clipboard contains the exact selected text")
        if op == "clipboard_stable":
            until = time.monotonic() + 2
            while time.monotonic() < until:
                assert ui.clipboard() == args[0], "Selection unexpectedly replaced the clipboard"
                time.sleep(0.1)
    elif op == "select_word":
        ui.select_word(*args)
    elif op == "copy":
        ui.copy()
    elif op == "paste":
        ui.chord("cmd+v" if ui.platform == "macos" else "ctrl+shift+v")
    elif op == "middle":
        ui.middle(*args)
    elif op == "drag":
        ui.drag(*args)
    elif op == "resize":
        ui.resize(*args)
    elif op == "display_scale":
        if args[0] == "high":
            ui.higher_display_scale()
        elif args[0] == "normal":
            ui.restore_display_scale()
        else:
            raise ValueError("Unknown native display scale " + args[0])
    elif op == "native_scale":
        assert ui.display_scale_state is not None, "No native display scale observation"

        def scaled():
            actual = ui.display_scale_state.observe()["scale"]
            return actual > 1.1 if args[0] == "high" else abs(actual - 1) < .05

        wait(scaled, "native output has " + args[0] + " scale")
    elif op == "native_reduced_motion":
        from .preferences import require_reduced_motion

        require_reduced_motion(ui)
    elif op == "geometry_probe":
        output = profile.repo / "geometry.json"
        script = Path(__file__).with_name("geometry_probe.py")
        jam.terminal()
        ui.type_shell_command(shlex.join([sys.executable, str(script), output.name, *map(str, args)]))
        ui.key("ENTER")
        wait(lambda: output.with_suffix(".ready").exists(), "PTY geometry observer startup")
        initial = json.loads(output.read_text())[0]
        assert initial["rows"] > 0 and initial["columns"] > 0
        memory["geometry"] = initial
    elif op == "interrupt_probe":
        output = profile.repo / "interrupt.json"
        script = Path(__file__).with_name("interrupt_probe.py")
        jam.terminal()
        ui.type_shell_command(shlex.join([sys.executable, str(script), output.name]))
        ui.key("ENTER")
        wait(output.with_suffix(".ready").exists, "real foreground job ready for Control-C")
        identity = json.loads(output.with_suffix(".ready").read_text())
        assert identity["pid"] > 1 and identity["group"] == identity["foreground"]
        memory["interrupt_job"] = identity
    elif op == "interrupted":
        output = profile.repo / "interrupt.json"
        wait(output.exists, "foreground job receives the terminal's SIGINT")
        assert json.loads(output.read_text()) == {**memory["interrupt_job"], "signal": signal.SIGINT}
    elif op == "geometry_changed":
        def changed():
            observations = json.loads((profile.repo / "geometry.json").read_text())
            return (len(observations) > 1 and observations[-1] != memory["geometry"]
                    and observations[-1]["rows"] > 0 and observations[-1]["columns"] > 0)

        wait(changed, "SIGWINCH and changed PTY rows/columns after native resize")
        execute(jam, ("geometry_stop",), memory)
    elif op in {"geometry_record", "geometry_matches", "geometry_differs"}:
        output = profile.repo / "geometry.json"

        def current():
            return json.loads(output.read_text())[-1]

        if op == "geometry_record":
            expected = memory.setdefault("geometry_sizes", {})[args[0]] = current()
        else:
            recorded = memory["geometry_sizes"][args[0]]
            wait(lambda: (current() == recorded) == (op == "geometry_matches"),
                 f"focused client PTY size {op} {args[0]}")
            expected = current()
        assert expected["rows"] > 0 and expected["columns"] > 0
        start = len(json.loads(output.read_text())) - 1
        until = time.monotonic() + 2
        while time.monotonic() < until:
            observations = json.loads(output.read_text())[start:]
            assert all(value == expected for value in observations), (
                "Inactive client changed the focused terminal's PTY geometry", observations)
            time.sleep(0.1)
    elif op == "geometry_stop":
        ui.type("exit")
        ui.key("ENTER")
        wait((profile.repo / "geometry.stopped").exists, "geometry observer exits through its terminal")
    elif op == "field_value":
        wait(lambda: ui.field_value(args[0]) == args[1], "field value")
    elif op == "file_exists":
        assert (profile.repo / args[0]).is_file()
    elif op == "path_absent":
        path = profile.repo / args[0]
        wait(lambda: not path.exists(), f"removed path {args[0]}")
    elif op == "agent_field":

        def matches():
            agents = profile.call("coordinate", "list")["Coordination"]["agents"]
            return len(agents) == 1 and agents[0].get(args[0]) == args[1]

        wait(matches, f"single agent {args[0]}={args[1]}")
    elif op == "registered_pane_fields":
        def matches():
            agents = profile.call("coordinate", "list")["Coordination"]["agents"]
            if len(agents) != 1:
                return False
            pane = next((p for p in profile.layout()["panes"] if p["id"] == agents[0]["pane"]), None)
            return pane and all(pane.get(field) == expected for field, expected in args[0].items())

        wait(matches, "registered agent's role and provider on its actual pane")
    elif op == "remember_pane":
        [pane] = profile.layout()["panes"]
        memory["pane"] = pane["id"]
    elif op == "remember_split_pane":
        [workspace] = profile.layout()["workspaces"]
        [tab] = workspace["tabs"]
        memory["pane"] = tab["root"]["Split"][args[0]]["Leaf"]
    elif op == "remembered_output":
        wait(lambda: args[0] in profile.capture(memory["pane"]),
             f"physical input reached the original pane: {args[0]}")
    elif op in {"pane_field", "remembered_pane_field"}:
        def matches():
            panes = profile.layout()["panes"]
            if op == "remembered_pane_field":
                panes = [pane for pane in panes if pane["id"] == memory["pane"]]
            return len(panes) == 1 and panes[0].get(args[0]) == args[1]

        wait(matches, f"pane {args[0]}={args[1]!r}")
    elif op == "release_program_completion":
        (profile.repo / "program-phase.done").touch()
    elif op == "program_identity":
        probe = Path(__file__).with_name("program_probe.py")
        output = profile.repo / "program-phase.json"
        jam.terminal()
        ui.type_shell_command(shlex.join([sys.executable, str(probe), args[0], output.name, *args[1:]]))
        ui.key("ENTER")
        wait(output.exists, "foreground program status receipt")
    elif op in {"set_progress", "reject_progress"}:
        [pane] = profile.layout()["panes"]
        options = [value for step in args for value in ("--step", step)] if args else ["--clear"]
        try:
            profile.call("status", "progress", "--pane", pane["id"], *options)
        except (subprocess.CalledProcessError, AssertionError) as error:
            if op != "reject_progress":
                raise
            message = error.stderr if isinstance(error, subprocess.CalledProcessError) else str(error)
            assert "distinct label" in message.lower(), message
        else:
            assert op == "set_progress", "The public CLI accepted duplicate step labels"
            expected = [{"label": name, "percent": int(percent)}
                        for name, percent in (step.rsplit("=", 1) for step in args)]
            wait(lambda: profile.layout()["panes"][0].get("progress", []) == expected,
                 "the foreground program's complete step list")
    elif op == "program_phase":
        ui.type(args[0])
        ui.key("ENTER")
        if args[0] != "exit":
            output = profile.repo / "program-phase.json"
            wait(lambda: json.loads(output.read_text())["phase"] == args[0],
                 "foreground program phase " + args[0])
    elif op in ("remember_agent_session", "same_agent_session"):
        agents = profile.call("coordinate", "list")["Coordination"]["agents"]
        assert len(agents) == 1 and agents[0]["session"]
        current = (agents[0]["id"], agents[0]["session"])
        if op == "remember_agent_session":
            memory["agent_session"] = current
            memory["agent_pane"] = agents[0]["pane"]
        else:
            assert current == memory["agent_session"], "Resume replaced the saved conversation"
    elif op == "agent_output":
        wait(lambda: args[0] in profile.capture(memory["agent_pane"]),
             f"resumed agent printed {args[0]} in its original pane")
    elif op == "provider_resumed_conversation":
        _, session = memory["agent_session"]
        events = profile.home / "provider-events.jsonl"

        def resumed():
            for line in events.read_text().splitlines(keepends=True):
                if not line.endswith("\n"):
                    continue
                argv = json.loads(line).get("argv", [])
                if "--resume" in argv and session in argv:
                    return True
            return False

        wait(resumed, "real provider process resumed the saved conversation")
    elif op == "one_launch_continuation":
        starts = [json.loads(line) for line in (profile.home / "provider-events.jsonl").read_text().splitlines()]
        resumed = [row["argv"] for row in starts if "--resume" in row.get("argv", [])]
        assert len(resumed) == 1
        assert sum(arg.startswith("Continue the interrupted work from this conversation.")
                   for arg in resumed[0]) == 1
        assert "fixture pending job" not in resumed[0]
    elif op == "worker_permission":
        events = profile.home / "provider-events.jsonl"

        def launched():
            if not events.exists():
                return None
            records = (json.loads(line) for line in events.read_text().splitlines(keepends=True)
                       if line.endswith("\n"))
            return next((r for r in records if r.get("provider") == "codex"
                         and r.get("agent") and r.get("argv")), None)
        row = wait(launched, "standalone worker provider launch")
        assert ("--yolo" in row["argv"]) == args[0]
    elif op == "no_registered_agents":
        wait(lambda: not profile.call("coordinate", "list")["Coordination"]["agents"],
             "the exited agent no longer owns the shell")
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
        command = shlex.join([sys.executable, str(probe), output.name, *args])
        jam.terminal()
        ui.type_shell_command(command)
        ui.key("ENTER")
        wait(lambda: output.with_suffix(".ready").exists(), "raw PTY probe ready")
    elif op == "probe_key":
        if "+" in args[0]:
            ui.chord(args[0])
        elif args[0].isupper():
            ui.key(args[0])
        else:
            ui.type(args[0])
    elif op == "begin_composition":
        ui.begin_composition()
    elif op == "finish_composition":
        ui.finish_composition(*args)
    elif op == "probe_no_input":
        events = profile.repo / "probe.events"
        finished = profile.repo / "probe.json"
        until = time.monotonic() + 1
        while time.monotonic() < until:
            assert not finished.exists(), "The raw PTY observer finished before checking input isolation"
            assert json.loads(events.read_text())["hex"] == "", "Unexpected input reached the PTY"
            time.sleep(.05)
    elif op == "preedit_pixels":
        assert ui.session.save_screenshot(str(profile.repo / "ime-preedit.png"))
    elif op == "probe_hex":
        output = profile.repo / "probe.json"
        wait(lambda: output.exists(), "raw PTY probe result")
        actual = json.loads(output.read_text())["hex"]
        assert actual == args[0], f"Raw PTY input {actual!r}, expected {args[0]!r}"
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
        if len(args) > 2:
            until = time.monotonic() + args[2]
            while time.monotonic() < until:
                assert path.read_text() == args[1], f"File {args[0]} changed after the operation"
                time.sleep(0.1)
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

        # macOS /var is a symlink to /private/var. The shell reports its
        # physical path, and terminal capture inserts newlines at soft wraps.
        expected = str(profile.repo.resolve()) if op == "output_repo" else args[0]

        def matches():
            text = "\n".join(profile.capture(p["id"]) for p in profile.layout()["panes"])
            if op == "output_repo":
                text = text.replace("\r", "").replace("\n", "")
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
    elif op == "remaining_pids":
        remaining = jam.pids()
        assert remaining and remaining.items() <= memory["pids"].items(), "An unaffected shell restarted"
    elif op == "remaining_original_pids":
        wait(lambda: memory["pids"].items() <= jam.pids().items(),
             "original shell survives the addition of a pane or tab")
    elif op == "remember_local_server":
        memory["local_server"] = json.loads((profile.socket.parent / "release.json").read_text())
    elif op == "stop_local_daemon":
        profile.call("daemon", "stop")
    elif op in {"restart_server", "local_server_replaced"}:
        receipt = profile.socket.parent / "release.json"
        if op == "restart_server":
            before = json.loads(receipt.read_text())
            ui.context("Local")
            ui.click("Restart Server…")
            ui.click("Restart Server", "button")
        else:
            before = memory["local_server"]

        def restarted():
            try:
                after = json.loads(receipt.read_text())
                if after["pid"] == before["pid"]:
                    return False
                os.kill(after["pid"], 0)
            except (FileNotFoundError, ProcessLookupError, json.JSONDecodeError):
                return False
            assert (after["build"], after["protocol"]) == (before["build"], before["protocol"])
            return after

        after = wait(restarted, "a new live daemon after the confirmed action")
        (profile.repo / "server-restart.json").write_text(json.dumps({"before": before, "after": after}))
        wait(lambda: ui.has("Terminal"), "fresh terminal after server restart")
    elif op == "git_branch":
        wait(
            lambda: args[0] in profile.git("branch", "--format=%(refname:short)").splitlines(),
            "created branch",
        )
    elif op == "shell_cwd":
        (name,) = args
        jam.run("pwd -P > " + shlex.quote(name))
        receipt = profile.repo / name
        wait(lambda: receipt.is_file() and receipt.read_text() == f"{profile.repo.resolve()}\n",
             "the shell started in the requested directory")
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
