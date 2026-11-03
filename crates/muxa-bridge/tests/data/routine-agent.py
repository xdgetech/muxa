#!/usr/bin/env python3
"""A fake agent for Routine runs (ADR 0026). Usage: routine-agent.py STEER_DIR

Each line typed into its pane is one turn. It sets @agent_state and
@agent_state_at on its pane, as tmux-agents does, and appends the line to
STEER_DIR/turns.log. Files in STEER_DIR steer it:
  first.json  copied to .muxa/routine/output.json on the start prompt
  fixed.json  copied there on the fail prompt
It also copies .muxa/routine/state.json, when present, to
STEER_DIR/seen-state-<n>.json, so a test sees what the run received.
"""

import os
import shutil
import subprocess
import sys
import time

PANE = os.environ["TMUX_PANE"]
STEER = sys.argv[1]
OUTPUT = ".muxa/routine/output.json"
STATE = ".muxa/routine/state.json"


def tmux(*args):
    subprocess.run(["tmux", *args], check=False)


def state(name):
    tmux(
        "set-option", "-p", "-t", PANE, "@agent_state", name, ";",
        "set-option", "-p", "-t", PANE, "@agent_state_at", str(int(time.time())),
    )


def copy(name):
    src = os.path.join(STEER, name)
    if os.path.exists(src):
        os.makedirs(os.path.dirname(OUTPUT), exist_ok=True)
        shutil.copyfile(src, OUTPUT)


def main():
    tmux("set-option", "-p", "-t", PANE, "@muxa_role", "agent")
    state("done")
    print("routine agent ready", flush=True)
    for line in sys.stdin:
        line = line.rstrip("\n")
        state("running")
        with open(os.path.join(STEER, "turns.log"), "a") as f:
            f.write(line + "\n")
        if "Read .muxa/TASK.md" in line:
            if os.path.exists(STATE):
                n = len([p for p in os.listdir(STEER) if p.startswith("seen-state-")])
                shutil.copyfile(STATE, os.path.join(STEER, f"seen-state-{n}.json"))
            copy("first.json")
        elif "did not pass the check" in line:
            copy("fixed.json")
        time.sleep(1)
        state("done")


if __name__ == "__main__":
    main()
