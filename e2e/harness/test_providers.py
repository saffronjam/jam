"""Run provider simulators as separate processes over their wire protocols."""

import json
import os
import subprocess
import sys
from pathlib import Path


def test_codex_protocol_retains_conversations(tmp_path):
    provision = Path(__file__).parents[1] / "fixtures/provision_provider.py"
    (tmp_path / "home").mkdir()
    (tmp_path / "bin").mkdir()
    subprocess.run([sys.executable, str(provision), str(tmp_path)], check=True, timeout=5)
    env = dict(os.environ, HOME=str(tmp_path / "home"))
    commands = [
        ("initialize", {}),
        ("config/read", {}),
        ("model/list", {}),
        ("thread/start", {}),
        ("thread/start", {"threadId": "known", "model": "fixture-model"}),
        ("thread/name/set", {"threadId": "known", "name": "Persisted name"}),
        ("thread/read", {"threadId": "known"}),
        ("thread/resume", {"threadId": "missing"}),
    ]
    result = subprocess.run(
        [str(tmp_path / "bin/codex"), "app-server"],
        input="".join(
            json.dumps({"id": i, "method": method, "params": params}) + "\n"
            for i, (method, params) in enumerate(commands)
        ),
        env=env,
        text=True,
        capture_output=True,
        check=True,
        timeout=5,
    )
    responses = [json.loads(line) for line in result.stdout.splitlines()]
    assert [value["id"] for value in responses] == list(range(len(commands)))
    assert responses[2]["result"]["data"][0]["model"] == "fixture-model"
    assert responses[6]["result"]["thread"]["name"] == "Persisted name"
    assert responses[7]["error"]["message"] == "saved conversation missing"
    resumed = subprocess.run(
        [str(tmp_path / "bin/codex"), "app-server"],
        input=json.dumps({"id": 1, "method": "thread/resume", "params": {"threadId": "known"}})
        + "\n",
        env=env,
        text=True,
        capture_output=True,
        check=True,
        timeout=5,
    )
    assert json.loads(resumed.stdout)["result"]["thread"]["name"] == "Persisted name"
