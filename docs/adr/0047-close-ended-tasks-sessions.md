# 0047: The daemon closes the sessions of ended Tasks

## Status

Proposed. It adds to ADR 0037 (an ended Task closes its session) and ADR 0019 (the host daemon).

## Context

- A Task closes its session once: when its status becomes `done` or `dropped` (ADR 0037). When that close fails, or tmux is not reachable, the session stays.
- tmux-resurrect restores the sessions of its last snapshot. A snapshot can be days old. It brings back the sessions of Tasks that ended after it was saved. `tmux/scripts/resurrect.py` restores their `@muxa_task`, so they are hidden from the Sessions list, but they still run and still show in the desktop tree.
- A snapshot from before `resurrect.py` has no tags. Its restored sessions show as manual sessions without a repo. This ADR does not cover them: without a tag, nothing ties a session to a Task safely.

## Decision

1. **The daemon closes every session whose `@muxa_task` names a `done` or `dropped` Task.** It runs at start and then with each worktree sweep (ADR 0043 §6), every 10 minutes, before the sweep.
2. **It closes the same way as ADR 0037**: `close_linked`, with mode `keep`. The worktree or folder stays. The sweep may then remove a worktree that nothing else holds.
3. **Only the tag decides.** A session without `@muxa_task`, or one tagged with an open Task, is never touched. Session names are never used (invariant 3).
4. A failed close is logged with the Task and session ids. The next sweep tries again.

## Consequences

- After a restore, the sessions of ended Tasks go at the next sweep: within 10 minutes, or at once when the daemon starts after tmux.
- When any Task has ended, the daemon attaches to tmux at each sweep, then detaches when it has no other work.
- A bridge without a daemon does not do this. A bridge still closes a session when its Task ends.
- Alternatives not taken:
  - Close on every 2-second tick: the daemon would stay attached to tmux with no other work.
  - A tmux-resurrect post-restore hook in muxa for tmux that kills them: the plugin never writes records, and closing an ended Task's session is the bridge's job (ADR 0037).
  - Link untagged sessions by pane path: a folder can hold several Tasks over time, and a manual session can work in a Task's folder.
