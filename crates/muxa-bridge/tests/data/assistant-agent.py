#!/usr/bin/env python3
"""A fake agent for assistant runs (ADR 0046). Usage: assistant-agent.py STEER_DIR

Each line typed into its pane is one turn. It sets @agent_state and
@agent_state_at on its pane, as tmux-agents does, and appends the line to
STEER_DIR/turns.log. Files in STEER_DIR steer it:
  first.json  written to .muxa/assistant/result.json on the start prompt
  fixed.json  written there on the fail prompt
  hold        present: it starts on a dialog (state running) until answered
              appears
  stall       present at the start prompt: the turn stays running until
              release appears, then writes first.json
A result's "request": "@" and "requestVersion": 0 are filled in from the
goal's last line in .muxa/TASK.md, `muxa-request: q-... v<n>`.
"""

import json
import os
import subprocess
import sys
import time

PANE = os.environ["TMUX_PANE"]
STEER = sys.argv[1]
RESULT = ".muxa/assistant/result.json"


def tmux(*args):
    subprocess.run(["tmux", *args], check=False)


def state(name):
    tmux(
        "set-option", "-p", "-t", PANE, "@agent_state", name, ";",
        "set-option", "-p", "-t", PANE, "@agent_state_at", str(int(time.time())),
    )


def steer(name):
    return os.path.exists(os.path.join(STEER, name))


def wait_for(name):
    while not steer(name):
        time.sleep(0.2)


def write(name):
    src = os.path.join(STEER, name)
    if not os.path.exists(src):
        return
    with open(src) as f:
        result = json.load(f)
    with open(".muxa/TASK.md") as f:
        line = [l for l in f.read().splitlines() if l.startswith("muxa-request: ")][-1]
    _, request, version = line.split(" ")
    if result.get("request") == "@":
        result["request"] = request
    if result.get("requestVersion") == 0:
        result["requestVersion"] = int(version[1:])
    os.makedirs(os.path.dirname(RESULT), exist_ok=True)
    with open(RESULT, "w") as f:
        json.dump(result, f)


def main():
    tmux("set-option", "-p", "-t", PANE, "@muxa_role", "agent")
    if steer("hold"):
        state("running")
        wait_for("answered")
    state("done")
    print("assistant agent ready", flush=True)
    for line in sys.stdin:
        line = line.rstrip("\n")
        state("running")
        with open(os.path.join(STEER, "turns.log"), "a") as f:
            f.write(line + "\n")
        if "Read .muxa/TASK.md" in line:
            if steer("stall"):
                wait_for("release")
            write("first.json")
        elif "did not pass the check" in line:
            write("fixed.json")
        time.sleep(1)
        state("done")


if __name__ == "__main__":
    main()
