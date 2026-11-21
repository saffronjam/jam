import json
import os
import shutil
import sys
import tempfile
import traceback
from pathlib import Path

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
    group.addoption(
        "--allow-subset",
        action="store_true",
        help="Development only: explicitly permit partial collection",
    )


def pytest_collection_modifyitems(config, items):
    config._jam_forbidden = []
    for item in items:
        if any(mark.name in {"skip", "skipif", "xfail"} for mark in item.iter_markers()):
            raise pytest.UsageError(f"Acceptance policy forbids skip/xfail: {item.nodeid}")
        item.add_marker("harness" if "harness" in item.path.parts else "acceptance")


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
            capture(item, jam)


def pytest_sessionfinish(session, exitstatus):
    if getattr(session.config, "_jam_policy_failure", False):
        session.exitstatus = 1
    if not session.testscollected:
        session.exitstatus = 5
    if getattr(session.config, "_jam_deselected", False) and not session.config.getoption(
        "--allow-subset"
    ):
        session.exitstatus = 1


def capture(item, jam):
    folder = Path(item.config.getoption("--artifacts-dir")).resolve() / item.name.replace("/", "_")
    folder.mkdir(parents=True, exist_ok=True)
    errors = []
    for name, operation in (
        ("ui.xml", jam.ui.tree),
        ("visible.txt", jam.ui.texts),
        ("layout.json", lambda: json.dumps(jam.profile.layout(), indent=2)),
    ):
        try:
            (folder / name).write_text(operation())
        except Exception as error:  # noqa: BLE001 - diagnostics must not mask the test failure
            errors.append(f"{name}: {type(error).__name__}: {error}")
    try:
        jam.ui.session.save_screenshot(str(folder / "screen.png"))
    except Exception as error:  # noqa: BLE001 - diagnostics must not mask the test failure
        errors.append(f"screenshot: {error}")
    log = jam.profile.state / "jamd.log"
    if log.exists():
        shutil.copyfile(log, folder / "jamd.log")
    try:
        for pane in jam.profile.layout()["panes"]:
            (folder / ("pane-" + pane["id"] + ".txt")).write_text(jam.profile.capture(pane["id"]))
    except Exception as error:  # noqa: BLE001 - diagnostics must not mask the test failure
        errors.append(f"pane captures: {error}")
    for source in (
        *jam.profile.root.glob("fixture-service-*.log"),
        *jam.profile.root.glob("*.events"),
        jam.profile.home / "provider-events.jsonl",
        jam.profile.root / "fixture-ids.json",
        jam.profile.root / "cli-events.jsonl",
    ):
        if source.is_file():
            shutil.copyfile(source, folder / source.name)
    (folder / "diagnostics-errors.txt").write_text("\n".join(errors))
    print(f"\nFailure artifacts: {folder}")


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
    # Short private paths also work with macOS's Unix socket path limit.
    with tempfile.TemporaryDirectory(prefix="je-") as temporary:
        profile = Profile(Path(temporary), Path(cli).resolve(), Path(daemon).resolve())
        ui = Desktop(platform, options.getoption("--webdriver-url"), app, profile)
        client = Jam(ui, profile)
        resources = None
        try:
            if case.fixture:
                from jam_e2e.bundle import prepare

                resources = prepare(options.getoption("--fixture-bundle"), case.fixture, profile)
            ui.launch()
            if not case.fixture:
                client.count("workspaces", 1)
                client.run("stty -echo")
            yield client
        except Exception:
            capture(request.node, client)
            raise
        finally:
            failures = []
            callbacks = [ui.restore_clipboard, ui.quit, profile.stop]
            if resources:
                callbacks.append(resources.stop)
            for cleanup in callbacks:
                try:
                    cleanup()
                except Exception:  # noqa: BLE001 - preserve all fixture cleanup failures
                    failures.append(traceback.format_exc())
            if failures:
                pytest.fail("Owned fixture cleanup failed:\n" + "\n".join(failures))
