import json
from pathlib import Path
import subprocess
import sys


def test_foreground_probe_emits_real_status_sequence_and_phase_receipt(tmp_path):
    probe = Path(__file__).parents[1] / "jam_e2e/program_probe.py"
    receipt = tmp_path / "phase.json"
    result = subprocess.run([sys.executable, str(probe), "claude-code", str(receipt)],
                            input="done\nclear\nexit\n", text=True, capture_output=True,
                            timeout=10, check=True)
    records = [f"\x1b]7501;state={state}:app=claude-code\x07"
               for state in ("working", "done", "clear")]
    assert all(record in result.stdout for record in records)
    assert [result.stdout.index(record) for record in records] == sorted(
        result.stdout.index(record) for record in records)
    assert json.loads(receipt.read_text()) == {"phase": "clear"}
    assert not receipt.with_suffix(".tmp").exists()
