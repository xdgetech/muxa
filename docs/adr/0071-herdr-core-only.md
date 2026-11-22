# 0071: muxa runs on herdr-core only

## Status

Accepted (2026-11-22). Completes ADR 0067. Supersedes ADR 0001, 0002, 0020, 0021 and 0055 (tmux control mode, window sizing, the tmux process guard), and the v1 parts of ADR 0015, 0018, 0019, 0034 and 0037.

## Context

ADR 0067 moved muxa to a session backend, with herdr-core as the default, and protocol v2. Phases A to E built the v2 path; the apps speak only v2 since D4 to D6. Until F, the daemon still carried the whole v1 host: protocol v1 over the bridge's stdin, tmux control mode, Task records with restart, handoff and park, the v1 Task Loop, Inbox items from pane observation, and the tmux-agents launcher. Two hosts in one binary meant every change touched both, and the v1 one could no longer be reached from any app.

The user decided (2026-11-22) what moves before the removal: Routine runs, the Reader, the Task Loop and Chat. v1 Task records, restart, handoff, park and the workers' v1 launch paths go without a port.

## Decision

1. **The daemon serves protocol v2 only, on herdr-core only.** `muxa-bridge` keeps `connect` and `daemon`; a bare `muxa-bridge`, as a forced key runs it, relays when the client asked for `connect` and otherwise prints its usage.
2. **Ported first, on herdr-core:**
   - Routine runs (F1): each run's agent is a terminal labelled `muxa.kind = routine`, with the start prompt as an argument; `done` runs the output check.
   - The Reader (F2): the files of a terminal's `muxa.dir` folder.
   - Task Loops (F3): the loop lives in the working place's `.muxa/loop.json`; each `done` runs the check.
   - Chat (F4): the agent's own records, found by its working folder.
3. **Removed:** protocol v1 (`protocol/PROTOCOL.md`, its fixtures and every v1 type), `muxa-tmux` and control mode, the launchers, Task records with restart, handoff, park and the worktree sweep, Inbox items from pane observation, Codex owned threads (ADR 0028's client), `muxa-cli` v1, the tmux findings and repro scripts, and the v1 client code in MuxaKit and Android.
4. **Compatibility kept:** `daemon run` reads an older service file's `--tmux`, `--socket`, `--idle-timeout` and `--task-launcher` and ignores them, so `make install`'s restart still starts the daemon. The state dir stays `~/.local/state/muxa/tasks`, so Routine and Inbox state carry over. `muxa-cli v2 …` still works.

## Consequences

- One host and one wire. Every client and test goes through protocol v2; the real server on the fake backend tests the Swift and Kotlin clients.
- Terminals are herdr-core's: muxa no longer sizes tmux windows or keeps a desktop layout (ADR 0020, 0021). A terminal takes the size of its last active viewer.
- A Task has no record: its identity is its agent terminal's labels and its files in the working place. Nothing restarts an agent with a handoff; a new Task in the same branch finds the running agent, or starts a new one.
- `daemon stop` no longer marks loops stopped; loops and Routine runs go on after the next start.
- Codex works as a Task's agent, but has no Chat and cannot run a Routine.

## Not decided

1. Whether Codex gets a folder lookup for Chat, and a Routine path without owned threads.
2. Whether a Task needs a record again, for example for its history after its agent ended.
