"""Run provider simulators as separate processes over their wire protocols."""

import json
import os
import select
import subprocess
import sys
from pathlib import Path

import pytest


def test_codex_protocol_retains_conversations(tmp_path):
    provision = Path(__file__).parents[1] / "fixtures/provision_provider.py"
    (tmp_path / "home").mkdir()
    (tmp_path / "bin").mkdir()
    # Provisioning includes a native compilation with its own 20-second limit.
    subprocess.run([sys.executable, str(provision), str(tmp_path)], check=True, timeout=30)
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


def test_native_provider_identity_and_signal_cleanup(tmp_path):
    provision = Path(__file__).parents[1] / "fixtures/provision_provider.py"
    (tmp_path / "home").mkdir()
    (tmp_path / "bin").mkdir()
    subprocess.run([sys.executable, str(provision), str(tmp_path)], check=True, timeout=30)
    executable = tmp_path / "bin/codex"
    process = subprocess.Popen(
        [str(executable), "app-server"], stdin=subprocess.PIPE, stdout=subprocess.PIPE,
        env=dict(os.environ, HOME=str(tmp_path / "home")), text=True,
    )
    try:
        process.stdin.write('{"id":1,"method":"initialize"}\n')
        process.stdin.flush()
        assert select.select([process.stdout], [], [], 5)[0], "Provider did not answer initialization"
        assert json.loads(process.stdout.readline())["id"] == 1
        arguments = subprocess.run(
            ["/bin/ps", "-ww", "-p", str(process.pid), "-o", "args="],
            check=True, capture_output=True, text=True, timeout=5,
        ).stdout.strip()
        assert arguments == str(executable) + " app-server"
        rows = subprocess.run(
            ["/bin/ps", "-A", "-o", "pid=,ppid="],
            check=True, capture_output=True, text=True, timeout=5,
        ).stdout.splitlines()
        children = [int(pid) for row in rows for pid, parent in [row.split()]
                    if int(parent) == process.pid]
        assert len(children) == 1
        process.terminate()
        process.wait(timeout=5)
        assert process.returncode != 0
        with pytest.raises(ProcessLookupError):
            os.kill(children[0], 0)
    finally:
        if process.poll() is None:
            process.terminate()
            process.wait(timeout=5)
        process.stdin.close()
        process.stdout.close()
