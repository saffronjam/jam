import json
import os
import shutil
import subprocess
import sys
import tempfile
import traceback
from pathlib import Path
from urllib.request import urlopen

import pytest

sys.path.insert(0, str(Path(__file__).parent))

from jam_e2e.driver import Desktop
from jam_e2e.profile import Profile
from jam_e2e.screens import Jam


def pytest_addoption(parser):
    group = parser.getgroup("Jam desktop acceptance")
    group.addoption(
        "--platform",
        choices=["macos", "linux"],
        default="macos" if sys.platform == "darwin" else "linux",
    )
    group.addoption("--app", help="Built .app on macOS, executable on Linux")
    group.addoption("--cli", help="Public jam CLI supplied with this installation")
    group.addoption("--daemon", help="jamd supplied with this installation")
    group.addoption("--webdriver-url", default="http://127.0.0.1:4723")
    group.addoption("--fixture-bundle", help="External service/SSH/release fixture manifest")
    group.addoption("--artifacts-dir", default=str(Path(__file__).parent / "artifacts"))
    group.addoption("--shard-index", type=int)
    group.addoption("--shard-count", type=int)
    group.addoption("--source-revision")
    group.addoption("--fixture-producer")
    group.addoption(
        "--allow-subset",
        action="store_true",
        help="Development only: explicitly permit partial collection",
    )


@pytest.hookimpl(trylast=True)
def pytest_collection_modifyitems(config, items):
    config._jam_forbidden = []
    for item in items:
        if any(mark.name in {"skip", "skipif", "xfail"} for mark in item.iter_markers()):
            raise pytest.UsageError(f"Acceptance policy forbids skip/xfail: {item.nodeid}")
        item.add_marker("harness" if "harness" in item.path.parts else "acceptance")
    index, count = config.getoption("--shard-index"), config.getoption("--shard-count")
    if index is None and count is None:
        return
    if index is None or count is None:
        raise pytest.UsageError("Both shard index and count are required")
    if config.getoption("--allow-subset") or config.option.keyword or config.option.markexpr:
        raise pytest.UsageError("Shards forbid --allow-subset, -k and -m")
    from jam_e2e.cases import CASES
    from jam_e2e.sharding import manifest

    ids = [case.id for case in CASES]
    collected = [getattr(getattr(item, "callspec", None), "params", {}).get("case")
                 for item in items]
    if [getattr(case, "id", None) for case in collected] != ids:
        raise pytest.UsageError("Sharding requires the complete shared catalog")
    try:
        receipt = manifest(ids, index, count, config.getoption("--platform"),
                           config.getoption("--source-revision"),
                           config.getoption("--fixture-producer"))
    except ValueError as error:
        raise pytest.UsageError(str(error)) from error
    folder = Path(config.getoption("--artifacts-dir"))
    folder.mkdir(parents=True, exist_ok=True)
    (folder / "shard.json").write_text(json.dumps(receipt, indent=2) + "\n")
    items[:] = items[index::count]


def pytest_deselected(items):
    # Checked in sessionfinish, including selection via -m and -k.
    for item in items:
        if "harness" not in item.path.parts:
            item.config._jam_deselected = True


@pytest.hookimpl(hookwrapper=True)
def pytest_runtest_makereport(item, call):
    result = yield
    report = result.get_result()
    if report.skipped or hasattr(report, "wasxfail"):
        item.config._jam_policy_failure = True
        report.outcome = "failed"
        report.longrepr = f"Forbidden skip/expected failure in {item.nodeid}: {report.longrepr}"
        if hasattr(report, "wasxfail"):
            del report.wasxfail
    if report.failed:
        jam = item.funcargs.get("jam")
        if jam:
            folder = Path(item.config.getoption("--artifacts-dir")).resolve() / item.name.replace("/", "_")
            if report.when == "teardown":
                folder /= "teardown"
            folder.mkdir(parents=True, exist_ok=True)
            # JUnit is finalized only when pytest exits. Retain the original
            # traceback even if later diagnostics or the full job are interrupted.
            (folder / "failure.txt").write_text(report.longreprtext + "\n")
            capture(item, jam, phase=report.when)


def pytest_sessionfinish(session, exitstatus):
    if getattr(session.config, "_jam_policy_failure", False):
        session.exitstatus = 1
    if not session.testscollected:
        session.exitstatus = 5
    if getattr(session.config, "_jam_deselected", False) and not session.config.getoption(
        "--allow-subset"
    ):
        session.exitstatus = 1


def capture(item, jam, phase="call"):
    key = "teardown" if phase == "teardown" else "primary"
    captured = getattr(item, "_jam_capture_phases", set())
    if key in captured:
        return
    captured.add(key)
    item._jam_capture_phases = captured
    folder = Path(item.config.getoption("--artifacts-dir")).resolve() / item.name.replace("/", "_")
    if phase == "teardown":
        folder /= "teardown"
    folder.mkdir(parents=True, exist_ok=True)
    (folder / "capture-started").write_text(phase + "\n")
    errors = []
    (folder / "driver-observations.json").write_text(json.dumps(jam.ui.observations, indent=2))
    ids_path = jam.profile.root / "fixture-ids.json"
    ids = {}
    if ids_path.is_file():
        try:
            ids = json.loads(ids_path.read_text())
            if "remote" in ids:
                from jam_e2e.remote import peer_call

                # Observe the peer independently of the app's cached status.
                # The existing trusted observer uses only its public CLI.
                peer = {"seed": ids, "current": peer_call(jam.profile, "daemon", "info")}
                archive = jam.profile.env.get("JAM_E2E_REMOTE_ARCHIVE")
                if archive:
                    builds = Path(archive).parent / "builds.json"
                    if builds.is_file():
                        peer["fixture_builds"] = json.loads(builds.read_text())
                (folder / "remote-host-info.json").write_text(json.dumps(peer, indent=2))
        except Exception as error:  # noqa: BLE001 - retain the original failure
            errors.append(f"independent peer information: {error}")
    if "remote" in ids:
        from jam_e2e.remote import peer_command, server_transactions

        try:
            (folder / "remote-ssh-journal.txt").write_text(peer_command(
                jam.profile,
                "sudo -n journalctl --no-pager -u ssh.service -u ssh.socket -n 100",
            ))
        except Exception as error:  # noqa: BLE001 - preserve the scenario failure
            errors.append(f"independent SSH service journal: {error}")
        try:
            (folder / "remote-server-transactions.json").write_text(
                json.dumps(server_transactions(jam.profile), indent=2))
            (folder / "remote-server-journal.txt").write_text(peer_command(
                jam.profile, "journalctl --user -u jamd.service --no-pager -n 100",
            ))
        except Exception as error:  # noqa: BLE001 - preserve the scenario failure
            errors.append(f"independent server transaction evidence: {error}")
    if sys.platform == "darwin":
        try:
            # Verify live native PIDs through the OS. logd can retain the first
            # path for identical Mach-O images copied into later profiles.
            roots = {str(jam.profile.root), str(jam.profile.root.resolve())}
            owners = [jam.ui, *(jam.clients.members.values() if jam.clients is not None else [])]
            pids = set()
            for ui in owners:
                assert ui.app.resolve().is_relative_to(jam.profile.root.resolve())
                pids.update(ui.application_pids())
            clauses = [f"processID == {pid}" for pid in sorted(pids)] if pids else [
                "processImagePath BEGINSWITH " + json.dumps(root.rstrip("/") + "/")
                for root in sorted(roots)
            ]
            predicate = "(" + " OR ".join(clauses) + (
                ') AND (eventMessage BEGINSWITH "Jam terminal geometry:" '
                'OR eventMessage BEGINSWITH "Jam server status:" '
                'OR eventMessage BEGINSWITH "Jam remote connection:" '
                'OR (subsystem == "dev.saffronjam.jam" AND category == "remote"))')
            (folder / "native-log-ownership.json").write_text(json.dumps({
                "pids": sorted(pids), "predicate": predicate,
            }, indent=2))
            result = subprocess.run(
                ["/usr/bin/log", "show", "--style", "json", "--last", "5m", "--predicate", predicate],
                check=True, capture_output=True, text=True, timeout=10,
            )
            (folder / "native-runtime.log").write_text(result.stdout)
        except Exception as error:  # noqa: BLE001 - diagnostics must preserve the test failure
            errors.append(f"native runtime logs: {error}")
    try:
        from jam_e2e.processes import owned_process_snapshots

        (folder / "processes.json").write_text(json.dumps(owned_process_snapshots(jam.profile), indent=2))
    except Exception as error:  # noqa: BLE001 - preserve the actual acceptance failure
        errors.append(f"owned process listing: {error}")
    receipt = jam.profile.socket.parent / "release.json"
    if receipt.is_file():
        shutil.copyfile(receipt, folder / "daemon-release.json")
        if sys.platform == "darwin":
            try:
                pid = json.loads(receipt.read_text())["pid"]
                subprocess.run(
                    [
                        "/usr/bin/sample",
                        str(pid),
                        "1",
                        "1",
                        "-file",
                        str(folder / "daemon-sample.txt"),
                    ],
                    check=True,
                    capture_output=True,
                    timeout=10,
                )
            except Exception as error:  # noqa: BLE001 - preserve the original failure
                errors.append(f"daemon sample: {error}")
    operations = [("layout.json", lambda: json.dumps(jam.profile.layout(), indent=2))]
    if jam.ui.platform == "macos" and jam.ui.dialog_active:
        errors.append("XCTest snapshots are ambiguous for the same-bundle native prompt; actual desktop OCR/PNGs retained.")
    else:
        operations += [("ui.xml", jam.ui.tree), ("visible.txt", jam.ui.texts)]
    for name, operation in operations:
        try:
            (folder / name).write_text(operation())
        except Exception as error:  # noqa: BLE001 - diagnostics must not mask the test failure
            errors.append(f"{name}: {type(error).__name__}: {error}")
    try:
        jam.ui.session.save_screenshot(str(folder / "screen.png"))
    except Exception as error:  # noqa: BLE001 - diagnostics must not mask the test failure
        errors.append(f"screenshot: {error}")
    if jam.clients is not None:
        (folder / "client-pids.json").write_text(json.dumps(jam.clients.pids, indent=2))
        for name, client_ui in jam.clients.members.items():
            try:
                jam.clients.select(name)
                target = folder / "clients" / name
                target.mkdir(parents=True, exist_ok=True)
                (target / "ui.xml").write_text(client_ui.tree())
                client_ui.session.save_screenshot(str(target / "screen.png"))
            except Exception as error:
                errors.append(f"client {name}: {error}")
    log = jam.profile.state / "jamd.log"
    if log.exists():
        shutil.copyfile(log, folder / "jamd.log")
    if jam.profile.config.exists():
        shutil.copyfile(jam.profile.config, folder / "config.toml")
    for name in ("probe.json", "probe.events", "probe.ready", "geometry.json", "geometry.ready",
                 "native-input-source.log",
                 "runner-network-consent.xml", "runner-network-consent.png"):
        source = jam.profile.repo / name
        if source.is_file():
            shutil.copyfile(source, folder / name)
    try:
        for transaction in (jam.profile.state / "upgrades").glob("apply-*"):
            for source in transaction.iterdir():
                if source.is_file() and source.suffix in {".json", ".log"}:
                    destination = folder / "upgrades" / transaction.name / source.name
                    destination.parent.mkdir(parents=True, exist_ok=True)
                    shutil.copyfile(source, destination)
    except Exception as error:  # noqa: BLE001 - retain the original acceptance failure
        errors.append(f"upgrade diagnostics: {error}")
    try:
        for pane in jam.profile.layout()["panes"]:
            try:
                text = jam.profile.capture(pane["id"])
                (folder / ("pane-" + pane["id"] + ".txt")).write_text(text)
            except Exception as error:
                errors.append(f"pane {pane['id']} capture: {error}")
    except Exception as error:  # noqa: BLE001 - diagnostics must not mask the test failure
        errors.append(f"pane captures: {error}")
    for source in (
        *jam.profile.root.glob("fixture-*.log"),
        *jam.profile.root.glob("*.events"),
        jam.profile.home / "provider-events.jsonl",
        jam.profile.root / "fixture-ids.json",
        jam.profile.root / "cli-events.jsonl",
    ):
        if source.is_file():
            shutil.copyfile(source, folder / source.name)
    (folder / "diagnostics-errors.txt").write_text("\n".join(errors))
    print(f"\nFailure artifacts: {folder}")


@pytest.fixture(scope="session")
def native_driver_ready(pytestconfig):
    endpoint = pytestconfig.getoption("--webdriver-url").rstrip("/")
    try:
        with urlopen(endpoint + "/status", timeout=3) as response:
            status = json.load(response)
        ready = status["value"]["ready"]
        # The pinned AT-SPI server serializes its ready boolean as text.
        assert ready is True or ready == "true", "WebDriver reports that it is not ready"
    except Exception as error:
        pytest.fail(f"Native WebDriver is unavailable before fixture provisioning: {error}")


@pytest.fixture
def jam(request, case):
    options = request.config
    platform = options.getoption("--platform")
    if platform != ("macos" if sys.platform == "darwin" else "linux"):
        pytest.fail("The native application must run on the selected operating system")
    app = options.getoption("--app")
    if not app or not Path(app).exists():
        pytest.fail(
            "Missing application artifact: supply --app. An absent Linux client is a failing setup, not a skip."
        )
    app = Path(app).resolve()
    from jam_e2e.bundle import installation_archive

    archive = installation_archive(options.getoption("--fixture-bundle"), case.fixture, platform)
    cli = options.getoption("--cli")
    daemon = options.getoption("--daemon")
    if platform == "macos":
        cli = cli or app / "Contents/Resources/bin/jam"
        daemon = daemon or app / "Contents/MacOS/jamd"
    else:
        cli = cli or app.parent / "jam"
        daemon = daemon or app.parent / "jamd"
    for binary in (cli, daemon):
        if not binary or not Path(binary).is_file() or not os.access(binary, os.X_OK):
            pytest.fail(f"Required executable missing: {binary}")
    # A missing automation service must fail before preparing real SSH VMs or
    # other worlds. Cache the session failure; every case still reports error.
    request.getfixturevalue("native_driver_ready")
    # Short private paths also work with macOS's Unix socket path limit.
    with tempfile.TemporaryDirectory(prefix="je-") as temporary:
        if platform == "macos":
            # A real updater replaces its installation. Each scenario owns a
            # private complete bundle outside the source checkout, so one
            # successful upgrade cannot change another case's starting build.
            original = app
            app = Path(temporary) / "installation/Jam.app"
            app.parent.mkdir()
            if archive is None:
                subprocess.run(["/usr/bin/ditto", str(original), str(app)], check=True, timeout=60)
            else:
                subprocess.run(["/usr/bin/ditto", "-xk", str(archive), str(app.parent)],
                               check=True, timeout=60)
                subprocess.run(["/usr/bin/codesign", "--verify", "--deep", "--strict", str(app)],
                               check=True, timeout=30)
            if not options.getoption("--cli"):
                cli = app / "Contents/Resources/bin/jam"
            if not options.getoption("--daemon"):
                daemon = app / "Contents/MacOS/jamd"
        profile = Profile(Path(temporary), Path(cli).resolve(), Path(daemon).resolve())
        ui = Desktop(platform, options.getoption("--webdriver-url"), app, profile)
        client = Jam(ui, profile)
        resources = None
        try:
            if case.fixture:
                from jam_e2e.bundle import prepare

                resources = prepare(options.getoption("--fixture-bundle"), case.fixture, profile)
            ui.launch(externally_owned=case.fixture in {"remote-password-required", "remote-host-untrusted"})
            if (platform == "macos" and os.environ.get("GITHUB_ACTIONS") == "true"
                    and "JAM_E2E_REMOTE_ARCHIVE" in profile.env):
                ui.allow_runner_network_consent()
                consent = profile.root / "runner-network-consent.xml"
                if consent.exists():
                    evidence = Path(options.getoption("--artifacts-dir")) / "native-environment" / case.id
                    evidence.mkdir(parents=True, exist_ok=True)
                    for extension in ("xml", "png"):
                        source = consent.with_suffix("." + extension)
                        if source.exists():
                            shutil.copy2(source, evidence / source.name)
            if platform == "macos" and not ui.external_app:
                ui.fit_initial_window()
            if not case.fixture:
                client.count("workspaces", 1)
                client.run("stty -echo")
            yield client
        except Exception:
            capture(request.node, client)
            raise
        finally:
            failures = []
            try:
                for kind in ("geometry", "interrupt", "server-restart"):
                    for source in profile.repo.glob(kind + ".*"):
                        if source.is_file():
                            evidence = Path(options.getoption("--artifacts-dir")) / kind / case.id
                            evidence.mkdir(parents=True, exist_ok=True)
                            shutil.copy2(source, evidence / source.name)
                for kind in ("visual", "dialog", "selection", "scale", "motion", "ime", "file-link",
                             "preview-frame"):
                    for source in profile.repo.glob(kind + "-*"):
                        if source.is_file():
                            evidence = Path(options.getoption("--artifacts-dir")) / kind / case.id
                            evidence.mkdir(parents=True, exist_ok=True)
                            shutil.copy2(source, evidence / source.name)
                for name in ("fixture-server-transition.log", "fixture-takeover-rollback.json",
                             "fixture-server-recovery.json"):
                    source = profile.root / name
                    if source.is_file():
                        evidence = Path(options.getoption("--artifacts-dir")) / "server" / case.id
                        evidence.mkdir(parents=True, exist_ok=True)
                        shutil.copy2(source, evidence / name)
            except Exception:
                failures.append("Native evidence retention:\n" + traceback.format_exc())
            callbacks = [profile.release_checkout, profile.resume_daemon, client.close_clients,
                         ui.restore_display_scale, ui.restore_input_source, ui.restore_clipboard,
                         ui.quit, profile.stop]
            if resources:
                callbacks.append(resources.stop)
            callbacks.append(profile.release_native_home)
            for cleanup in callbacks:
                try:
                    cleanup()
                except Exception:  # noqa: BLE001 - preserve all fixture cleanup failures
                    failures.append(traceback.format_exc())
            if (profile.root / "ime.ready").exists():
                try:
                    evidence = Path(options.getoption("--artifacts-dir")) / "native-ime" / case.id
                    evidence.mkdir(parents=True, exist_ok=True)
                    for name in ("ime.ready", "fixture-ime-cleanup.json"):
                        source = profile.root / name
                        if source.is_file():
                            shutil.copy2(source, evidence / name)
                    (evidence / "observations.json").write_text(json.dumps(ui.observations, indent=2))
                except Exception:
                    failures.append("Native IME evidence retention:\n" + traceback.format_exc())
            if failures:
                pytest.fail("Owned fixture cleanup failed:\n" + "\n".join(failures))
