"""Helper of agent-usage.sh. Keeps only rate limit fields: never a transcript,
a prompt, a path from the agent's input or a credential."""

import glob
import json
import os
import subprocess
import sys
import time

ROLLOUTS_TO_SCAN = 20
WINDOW_KEYS = ("used_percentage", "resets_at", "used_percent", "window_minutes")


def windows(limits):
    """The numeric fields of each window, nothing else."""
    out = {}
    for name, value in (limits or {}).items():
        if isinstance(value, dict):
            out[name] = {k: value[k] for k in WINDOW_KEYS if k in value}
        elif name in ("plan_type", "limit_id", "rate_limit_reached_type"):
            out[name] = value
    return out


def own_status_line(home):
    """The login's own status line command, from its settings.json."""
    try:
        with open(os.path.join(home, "settings.json")) as f:
            line = json.load(f).get("statusLine") or {}
    except (OSError, ValueError):
        return None
    return line.get("command") if line.get("type") == "command" else None


def statusline(home, out_dir):
    raw = sys.stdin.read()
    try:
        data = json.loads(raw)
    except ValueError:
        data = {}
    record = {
        "at": int(time.time()),
        "has_rate_limits": "rate_limits" in data,
        "rate_limits": windows(data.get("rate_limits")),
    }
    command = own_status_line(home)
    record["own_status_line"] = command is not None
    printed = "muxa probe"
    if command:
        run = subprocess.run(
            ["/bin/sh", "-c", command], input=raw, capture_output=True, text=True
        )
        record["own_status_line_exit"] = run.returncode
        record["own_status_line_printed"] = bool(run.stdout.strip())
        printed = run.stdout.rstrip("\n") or printed
    with open(os.path.join(out_dir, "reports.jsonl"), "a") as f:
        f.write(json.dumps(record) + "\n")
    print(printed)


def report(out_dir):
    path = os.path.join(out_dir, "reports.jsonl")
    if not os.path.exists(path):
        print("FAIL  the status line command never ran under --settings")
        return 1
    with open(path) as f:
        reports = [json.loads(line) for line in f if line.strip()]
    last = reports[-1]
    print(f"PASS  the status line command ran {len(reports)} times under --settings")
    verdict = "PASS" if last["has_rate_limits"] else "INFO"
    print(f"{verdict}  rate_limits present: {last['has_rate_limits']}")
    if not last["has_rate_limits"] and len(reports) < 2:
        print("      the status line ran only before the first reply; send a message and wait for it")
    print("      " + json.dumps(last["rate_limits"]))
    if last["own_status_line"]:
        ok = last.get("own_status_line_exit") == 0 and last.get("own_status_line_printed")
        print(("PASS" if ok else "FAIL") + "  the login's own status line ran and printed")
    else:
        print("INFO  this login has no status line of its own")
    return 0


def last_rate_limits(path):
    last = None
    with open(path) as f:
        for line in f:
            try:
                event = json.loads(line)
            except ValueError:
                continue
            payload = event.get("payload") or {}
            if payload.get("type") == "token_count" and payload.get("rate_limits"):
                last = payload["rate_limits"]
    return last


def codex(home):
    rollouts = sorted(
        glob.glob(os.path.join(home, "sessions", "**", "rollout-*.jsonl"), recursive=True),
        key=os.path.getmtime,
    )
    if not rollouts:
        print("FAIL  no rollout under sessions/; run one codex turn with this home")
        return 1
    # A rollout without a model reply has no token_count; look back a little.
    for path in reversed(rollouts[-ROLLOUTS_TO_SCAN:]):
        last = last_rate_limits(path)
        if last is not None:
            break
    else:
        print(f"FAIL  none of the newest {ROLLOUTS_TO_SCAN} rollouts has rate_limits")
        return 1
    print("PASS  rate_limits in a recent rollout")
    print("      " + json.dumps(windows(last)))
    return 0


def main():
    mode = sys.argv[1]
    if mode == "statusline":
        statusline(sys.argv[2], sys.argv[3])
        return 0
    if mode == "report":
        return report(sys.argv[2])
    if mode == "codex":
        return codex(sys.argv[2])
    return 2


if __name__ == "__main__":
    sys.exit(main())
