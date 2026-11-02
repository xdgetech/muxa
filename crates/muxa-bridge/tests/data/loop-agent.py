#!/usr/bin/env python3
"""A fake agent for the Task Loop tests (ADR 0019 section 7).

Each line typed into its pane is one turn. It sets @agent_state and
@agent_state_at on its pane, as tmux-agents does, and appends the line to
turns.log in its directory. Files in that directory steer it:
  end    "done" (the default, tmux-agents while the user is away) or "idle"
  fix    a fail prompt makes the turn create "fixed"
  wait   the next turn goes "waiting" until one more line arrives
  error  the next turn ends in "error"
  busy   at start, the agent reports "running" instead of a finished turn
Each turn also appends "<epoch seconds> <line>" to times.log.
It is Python, not sh, so tmux does not see a shell in the pane's foreground.
"""

import os
import subprocess
import sys
import time

PANE = os.environ["TMUX_PANE"]
FAIL_PROMPT = "The check failed."


def tmux(*args):
    subprocess.run(["tmux", *args], check=False)


def state(name):
    tmux(
        "set-option", "-p", "-t", PANE, "@agent_state", name, ";",
        "set-option", "-p", "-t", PANE, "@agent_state_at", str(int(time.time())),
    )


def take(name):
    if os.path.exists(name):
        os.remove(name)
        return True
    return False


def ending():
    try:
        with open("end") as f:
            return f.read().strip() or "done"
    except FileNotFoundError:
        return "done"


def main():
    tmux("set-option", "-p", "-t", PANE, "@muxa_role", "agent")
    state("running" if os.path.exists("busy") else ending())
    print("fake agent ready", flush=True)
    for line in sys.stdin:
        line = line.rstrip("\n")
        state("running")
        with open("turns.log", "a") as f:
            f.write(line + "\n")
        with open("times.log", "a") as f:
            f.write(f"{int(time.time())} {line}\n")
        if take("wait"):
            state("waiting")
            answer = sys.stdin.readline()
            with open("turns.log", "a") as f:
                f.write("answer: " + answer)
            state("running")
        if FAIL_PROMPT in line and os.path.exists("fix"):
            open("fixed", "w").close()
        time.sleep(1)
        state("error" if take("error") else ending())


if __name__ == "__main__":
    main()
