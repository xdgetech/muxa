#!/usr/bin/env python3
"""A fake agent for assistant runs (ADR 0048). Usage: assistant-agent.py STEER_DIR,
or, as a staff member's worker (ADR 0051): assistant-agent.py --agent NAME with
ASSISTANT_STEER naming STEER_DIR.

Each line typed into its pane is one turn. It sets @agent_state and
@agent_state_at on its pane, as tmux-agents does, and appends the line to
STEER_DIR/turns.log. Files in STEER_DIR steer it:
  first.json  written to .muxa/assistant/result.json on the start prompt
  fixed.json  written there on the fail prompt
  hold        present: it starts on a dialog (state running) until answered
              appears
  trust       present: it shows Claude Code's trust dialog, with no state,
              until answered appears
  stall       present at the start prompt: the turn stays running until
              release appears, then writes first.json
A member NAME reads first.NAME.json and fixed.NAME.json before the shared
files, stalls on stall.NAME as on stall, and logs its turns as "NAME: <line>". With CLAUDE_CONFIG_DIR set, it
keeps a Claude Code session file there: a named worker's run is delivered
only to a process the adapter binds (ADR 0027).
A result's "request": "@" and "requestVersion": 0 are filled in from the
goal's last line in .muxa/TASK.md, `muxa-request: q-... v<n>` or
`muxa-request: q-... v<n> s<k>`. An overall
run's goal ends with `muxa-plan: p-...`: the file is written to
.muxa/assistant/plan.json instead, with "plan": "@" filled in.
"""

import json
import os
import subprocess
import sys
import time

PANE = os.environ["TMUX_PANE"]
ARGS = sys.argv[1:]
NAME = ARGS[ARGS.index("--agent") + 1] if "--agent" in ARGS else None
STEER = os.environ.get("ASSISTANT_STEER") if NAME else ARGS[0]
RESULT = ".muxa/assistant/result.json"
PLAN = ".muxa/assistant/plan.json"


def tmux(*args):
    """Runs on the server of the pane, which $TMUX names."""
    return subprocess.run(["tmux", *args], check=False, capture_output=True, text=True)


def state(name):
    tmux(
        "set-option", "-p", "-t", PANE, "@agent_state", name, ";",
        "set-option", "-p", "-t", PANE, "@agent_state_at", str(int(time.time())),
    )
    session_file("busy" if name == "running" else "idle")


def session_file(status):
    """A Claude Code session file, so a named worker's process is bound."""
    home = os.environ.get("CLAUDE_CONFIG_DIR")
    if not home:
        return
    place = tmux("display-message", "-p", "-t", PANE,
                 "#{session_name}:#{window_id}.#{pane_id}").stdout.strip()
    os.makedirs(os.path.join(home, "sessions"), exist_ok=True)
    now = int(time.time() * 1000)
    record = {"pid": os.getpid(), "sessionId": "0000feed-%08x" % os.getpid(),
              "cwd": os.getcwd(), "startedAt": now, "kind": "interactive",
              "tmux": place, "status": status, "updatedAt": now}
    path = os.path.join(home, "sessions", str(os.getpid()) + ".json")
    with open(path + ".tmp", "w") as f:
        json.dump(record, f)
    os.replace(path + ".tmp", path)


def steer(name):
    return os.path.exists(os.path.join(STEER, name))


def wait_for(name):
    while not steer(name):
        time.sleep(0.2)


def write(name):
    src = os.path.join(STEER, name)
    if NAME:
        mine = os.path.join(STEER, name.replace(".json", "." + NAME + ".json"))
        if os.path.exists(mine):
            src = mine
    if not os.path.exists(src):
        return
    with open(src) as f:
        result = json.load(f)
    with open(".muxa/TASK.md") as f:
        lines = f.read().splitlines()
    plans = [l for l in lines if l.startswith("muxa-plan: ")]
    if plans:
        target = PLAN
        if result.get("plan") == "@":
            result["plan"] = plans[-1].split(" ")[1]
    else:
        target = RESULT
        line = [l for l in lines if l.startswith("muxa-request: ")][-1]
        _, request, version = line.split(" ")[:3]
        if result.get("request") == "@":
            result["request"] = request
        if result.get("requestVersion") == 0:
            result["requestVersion"] = int(version[1:])
    os.makedirs(os.path.dirname(target), exist_ok=True)
    with open(target, "w") as f:
        json.dump(result, f)


def main():
    tmux("set-option", "-p", "-t", PANE, "@muxa_role", "agent")
    if steer("trust"):
        # As Claude Code 2.1.322 draws it (docs/findings/claude-code-trust.md).
        print(" Quick safety check: Is this a project you created or one you trust?")
        print(" ❯ No, exit")
        print("   Yes, I trust this folder", flush=True)
        wait_for("answered")
    if steer("hold"):
        state("running")
        wait_for("answered")
    state("done")
    print("assistant agent ready", flush=True)
    for line in sys.stdin:
        line = line.rstrip("\n")
        state("running")
        with open(os.path.join(STEER, "turns.log"), "a") as f:
            f.write((NAME + ": " if NAME else "") + line + "\n")
        if "Read .muxa/TASK.md" in line:
            if steer("stall") or (NAME and steer("stall." + NAME)):
                wait_for("release")
            write("first.json")
        elif "did not pass the check" in line:
            write("fixed.json")
        time.sleep(1)
        state("done")


if __name__ == "__main__":
    main()
