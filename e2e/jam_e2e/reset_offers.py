"""Control isolated providers and verify their external reset receipts."""

import json
import uuid


def offer(profile, kind):
    for path in (profile.home / "fixture-world.json", profile.root / "service-world.json"):
        state = json.loads(path.read_text())
        state["reset_offer"] = kind
        tmp = path.with_suffix(".pending")
        tmp.write_text(json.dumps(state))
        tmp.replace(path)


def events(profile, provider):
    path = (profile.home / "provider-events.jsonl" if provider == "codex"
            else profile.root / "https.events")
    if not path.exists():
        return []
    return [json.loads(line) for line in path.read_text().splitlines(keepends=True)
            if line.endswith("\n")]


def receipts(profile, provider, count, kind):
    if provider == "claude":
        claims = [e for e in events(profile, provider) if e.get("grant")]
        assert all(e["grant"] == "fixture-grant" and e["result"] == "reset" for e in claims)
        keys = [entry["attempt"] for entry in claims]
    else:
        claims = [e["params"] for e in events(profile, provider)
                  if e.get("method") == "account/rateLimitResetCredit/consume"]
        assert all(e.get("creditId") == (None if kind == "count" else "fixture-credit") for e in claims)
        keys = [entry["idempotencyKey"] for entry in claims]
    assert len(keys) == count, f"{provider} received {len(keys)} reset calls, expected {count}"
    if keys:
        assert len({uuid.UUID(key) for key in keys}) == 1, "Retry changed the provider attempt ID"


def reads(profile):
    return sum(e.get("method") == "account/rateLimits/read" for e in events(profile, "codex"))
