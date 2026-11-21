"""Deterministic external Claude/Codex processes implementing their public protocols.

This program is copied to private bin/claude and bin/codex. It never imports Jam.
Its state and control files belong to the fixture, not to the application.
"""

import fcntl
import json
import os
import re
import socket
import subprocess
import sys
import threading
import uuid
from pathlib import Path

HOME = Path(os.environ["HOME"])
WORLD = Path(os.environ.get("JAM_E2E_WORLD", HOME / "fixture-world.json"))
PROVIDER = Path(sys.argv[0]).name


def world():
    return json.loads(WORLD.read_text()) if WORLD.exists() else {}


def record(value):
    with (HOME / "provider-events.jsonl").open("a") as output:
        fcntl.flock(output, fcntl.LOCK_EX)
        output.write(json.dumps(dict(provider=PROVIDER, **value)) + "\n")


def threads(update=None):
    path = HOME / "provider-threads.json"
    with (HOME / "provider-threads.lock").open("a") as lock:
        fcntl.flock(lock, fcntl.LOCK_EX)
        data = json.loads(path.read_text()) if path.exists() else {}
        if update:
            data.update(update)
            temporary = path.with_suffix(".tmp")
            temporary.write_text(json.dumps(data))
            temporary.replace(path)
        return data


def models():
    return [
        {
            "model": "fixture-model",
            "displayName": "Fixture model",
            "isDefault": True,
            "supportedReasoningEfforts": [{"reasoningEffort": "high"}],
        }
    ]


def codex_rpc():
    for line in sys.stdin:
        request = json.loads(line)
        if "id" not in request:
            continue
        method, params = request["method"], request.get("params", {})
        record({"method": method, "params": params})
        response = {"id": request["id"]}
        result = {}
        if world().get("helper_failure"):
            response["error"] = {"message": "fixture-launch-error"}
        elif method == "config/read":
            result = {
                "config": {
                    "model": "fixture-model",
                    "developer_instructions": "Fixture instructions",
                }
            }
        elif method == "model/list":
            result = {"data": models(), "nextCursor": None}
        elif method in ("thread/start", "thread/resume"):
            identifier = params.get("threadId", str(uuid.uuid4()))
            if method == "thread/resume" and identifier not in threads():
                response["error"] = {"message": "saved conversation missing"}
            else:
                thread = threads().get(identifier) or {
                    "id": identifier,
                    "name": "fixture-conversation",
                    "cwd": os.getcwd(),
                    "preview": "Fixture history",
                }
                threads({identifier: thread})
                result = {"thread": thread, "model": params.get("model", "fixture-model")}
        elif method == "thread/name/set":
            thread = threads()[params["threadId"]]
            thread["name"] = params["name"]
            threads({thread["id"]: thread})
        elif method == "thread/read":
            result = {"thread": threads()[params["threadId"]]}
        elif method == "thread/list":
            result = {
                "data": [value for value in threads().values() if value["cwd"] == params.get("cwd")]
            }
        elif method != "initialize":
            response["error"] = {"message": "Unsupported fixture RPC: " + method}
        if "error" not in response:
            response["result"] = result
        print(json.dumps(response), flush=True)


def claude_catalog():
    for line in sys.stdin:
        request = json.loads(line)
        record({"request": request})
        result = {
            "models": [
                {
                    "value": "default",
                    "resolvedModel": "fixture-model",
                    "displayName": "Fixture model",
                    "supportedEffortLevels": ["high"],
                }
            ]
        }
        print(
            json.dumps(
                {
                    "type": "control_response",
                    "response": {
                        "request_id": request["request_id"],
                        "subtype": "success",
                        "response": result,
                    },
                }
            ),
            flush=True,
        )


def jam(*args, data=None):
    result = subprocess.run(
        [os.environ["JAM_E2E_CLI"], "--json", *args],
        input=json.dumps(data) if data else None,
        text=True,
        capture_output=True,
        timeout=15,
        check=True,
    )
    return json.loads(result.stdout) if result.stdout.strip().startswith("{") else result.stdout


def hook(event, session, prompt=""):
    jam(
        "skill",
        "context",
        "--agent",
        PROVIDER,
        data={
            "hook_event_name": event,
            "source": "startup",
            "session_id": session,
            "prompt": prompt,
            "model": "fixture-model",
        },
    )


def accept_message(message, session):
    record({"message": message})
    state = world()
    if state.get("acknowledge", True):
        inbox = jam("coordinate", "inbox", "--agent", os.environ["JAM_AGENT_ID"])
        messages = inbox.get("Coordination", [])
        for item in messages:
            jam("coordinate", "ack", item["id"], "--agent", os.environ["JAM_AGENT_ID"])
            if state.get("report_result") and item.get("task"):
                board = jam("coordinate", "list", "--all")["Coordination"]
                task = next(t for t in board["tasks"] if t["id"] == item["task"])
                if (
                    task.get("assignee") == os.environ["JAM_AGENT_ID"]
                    and task["state"] == "assigned"
                ):
                    jam(
                        "coordinate",
                        "result",
                        "--task",
                        item["task"],
                        "--agent",
                        os.environ["JAM_AGENT_ID"],
                        "fixture-result",
                    )
    hook("UserPromptSubmit", session, message)
    hook("Stop", session)


def interactive():
    record({"argv": sys.argv[1:], "agent": os.environ.get("JAM_AGENT_ID")})
    if world().get("launch_failure"):
        print("fixture-launch-error", flush=True)
        raise SystemExit(2)
    session = sys.argv[-1] if len(sys.argv) > 1 else str(uuid.uuid4())
    if PROVIDER == "claude":
        folder = Path(os.environ.get("CLAUDE_CONFIG_DIR", HOME / ".claude")) / "projects/fixture"
        folder.mkdir(parents=True, exist_ok=True)
        (folder / (session + ".jsonl")).write_text(
            json.dumps({"type": "user", "message": "Fixture history"}) + "\n"
        )
        listener = socket.socket(socket.AF_UNIX)
        # Keep the path below Darwin's Unix socket length limit.
        path = HOME.parent / ("peer-" + session[:12])
        path.unlink(missing_ok=True)
        listener.bind(str(path))
        listener.listen()
        os.environ["CLAUDE_CODE_MESSAGING_SOCKET"] = str(path)
        os.environ["CLAUDE_CODE_MESSAGING_TOKEN"] = "fixture-only"

        def receive():
            while True:
                connection, _ = listener.accept()
                with connection, connection.makefile() as stream:
                    auth = json.loads(stream.readline())
                    if auth != {"type": "auth", "token": "fixture-only"}:
                        raise ValueError("Unexpected fixture messaging authentication")
                    message = json.loads(stream.readline())["message"]["content"]
                    if not world().get("hold_delivery"):
                        accept_message(message, session)

        threading.Thread(target=receive, daemon=True).start()
    if not world().get("hold_readiness"):
        hook("SessionStart", session)
    print("fixture-agent-resumed", flush=True)
    for line in sys.stdin:
        if "exit" == line.strip():
            hook("SessionEnd", session)
            return
        if line.strip():
            hook("UserPromptSubmit", session, line.strip())
            hook("Stop", session)


def main():
    if "--version" in sys.argv:
        print(PROVIDER + " fixture-1.0")
    elif PROVIDER == "codex" and "app-server" in sys.argv:
        codex_rpc()
    elif PROVIDER == "claude" and "--output-format" in sys.argv:
        claude_catalog()
    elif PROVIDER == "codex" and len(sys.argv) > 1 and sys.argv[1] == "queue":
        if world().get("queue_failure"):
            raise SystemExit(2)
        message = sys.argv[sys.argv.index("--message") + 1]
        agent = re.search(r"--agent ([a-f0-9-]+)", message)
        if agent:
            os.environ["JAM_AGENT_ID"] = agent[1]
        if not world().get("hold_delivery"):
            accept_message(message, sys.argv[sys.argv.index("--thread") + 1])
        record({"queued": message})
    else:
        interactive()


if __name__ == "__main__":
    main()
