"""Fault injection must recognize real process arguments through a symlinked home."""

import json
import signal
import subprocess
import sys
import tempfile
from pathlib import Path

from fixtures.interrupt_upgrade import interrupt


def test_interrupt_uses_the_public_journal_path_in_process_arguments(monkeypatch):
    with tempfile.TemporaryDirectory(prefix="jhu-") as directory:
        parent = Path(directory)
        actual = parent / "actual"
        actual.mkdir()
        public = parent / "alias"
        public.symlink_to(actual, target_is_directory=True)
        journal = public / "state/upgrades/apply-fixture/journal.json"
        journal.parent.mkdir(parents=True)
        journal.write_text('{"phase":"staged"}')
        # A real child retains the symlink spelling in argv, just like exec
        # through macOS's /var alias. It has no application or updater code.
        (parent / "__upgrade-apply").write_text("import time; time.sleep(30)\n")
        child = subprocess.Popen(
            [str(journal.parent / "jam-updater"), "__upgrade-apply", str(journal)],
            executable=sys.executable,
            cwd=parent,
        )
        run = subprocess.run

        def cli_or_ps(args, **options):
            if args[0] == "fixture-cli":
                return subprocess.CompletedProcess(
                    args, 0, stdout=json.dumps({"started": True, "journal": str(journal)})
                )
            return run(args, **options)

        monkeypatch.setenv("JAM_E2E_CLI", "fixture-cli")
        monkeypatch.setattr(subprocess, "run", cli_or_ps)
        try:
            interrupt(public)
            child.wait(timeout=5)
            assert child.returncode == -signal.SIGKILL
            event = json.loads((public / "fixture-upgrade-interruption.log").read_text())
            assert event["pid"] == child.pid and event["journal"] == str(journal)
        finally:
            if child.poll() is None:
                child.kill()
                child.wait(timeout=5)
