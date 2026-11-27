# 0104: Pause and resume a Task

Status: accepted (2026-11-26). It answers ADR 0071 "Not decided" 2 for paused Tasks only.

## Context

- Since ADR 0071, a Task has no record. Its identity is its agent terminal's labels. `terminals` lists live terminals only, and every app lists only those.
- The user closed an agent to free memory during a long review wait, and the Task left the sidebar: "I only wanted to stop the session for resources for a while."
- The only way back was `start-task` with an `existing` place. It needs the path, and it loses the title, the Task Loop, the Bot of a talk and the link from the open Inbox items.
- v1 had park (ADR 0034). ADR 0071 removed it without a port.

## Decision

1. **Pause** (`pause-task`) stops a Task's agent and its shells and keeps the Task:
   1. The host writes a record of the agent.
   2. A live Task Loop stops with reason `user`.
   3. The agent is killed and its terminal removed. A kill makes no Failed item.
   4. Its shells end.

   The worktree, the branch, the folder and the open Inbox items stay. Nothing checks for work that could be lost, because nothing is removed.
2. **The record lives in the daemon's state dir**, `<state dir>/paused.json`, beside `inbox.json` and `usage.json`. It is the agent terminal as it was: its id, name and every `muxa.*` label, plus the pause time and the check of a loop that was active. It survives a daemon restart, a herdr-core restart and a reboot.
3. **A paused Task keeps its agent's terminal id.** Open Inbox items already point at it, so the apps match them with no new field. Every app lists a paused Task with the live ones, marked paused.
4. **Resume** (`resume-task`) starts the Workspace's agent again from the record, in the same places with the same labels and title:
   - Claude Code gets `--continue` when its config dir holds a conversation for the folder, as `start-task` does for an `existing` place.
   - The agent gets the continue prompt of ADR 0098. A talk gets none, as on Retry.
   - Another `claude` or `codex` process in the folder is `conflict`, and nothing starts.
   - A loop that was active is armed again with its check and its attempts kept.
   - Open items move from the old terminal id to the new one.
   - Plan mode is not kept, as on Retry.
5. **Any agent start in a paused Task's working place ends the pause.** Resume, `start-task` with an `existing` place and Retry all drop the record and move its items to the new agent. Two records for one place can never be live together.
6. **Close Task closes a paused Task too.** `close-task` with the paused id checks and removes the places as for a live Task, marks its items Done and drops the record.
7. **A working place that is gone** makes Resume `bad-request`. The record stays, so Close can still remove the Task.

## Consequences

- Pausing frees the agent's process and its shells. The disk stays used.
- A Task now has a record again, but only while it is paused. A live Task is still its labels.
- The apps get the paused list with `paused-tasks` and keep it current from the `paused` notification.
- iOS and Android get a row menu for their Task lists, and a paused Task's screen offers Resume and Close Task. Before this, Android had no Close Task outside Approve.

## Not decided

- Pausing a Task by itself after it has been idle for a while (ADR 0034's `park_after`). The setting stays parsed and unused.
- Pausing a Routine run or a team member.

## Alternatives not taken

- **Keep the ended terminal with a label.** `terminals` does not list ended terminals, and herdr-core loses them when it restarts.
- **A `.muxa/paused.json` in the working place.** Listing paused Tasks would need a scan of every Workspace and worktree.
- **A "Resume:" Inbox item, as the v1 migration made (ADR 0076).** The Task leaves the sidebar, and the item's Resume loses the loop, the Bot and the item links.
