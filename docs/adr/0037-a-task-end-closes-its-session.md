# 0037: When a Task ends, its session closes

## Status

Proposed. The developer chose this on 2026-11-04. ADR 0043 supersedes §1's "the worktree or folder stays": a worktree goes when nothing holds it. It adds to ADR 0033 (Accept marks the Task done) and ADR 0026 (Routine runs). It follows the direction of ADR 0034 §4: a session can close while the Task's record and files stay.

## Context

- Done and Drop change a Task's status only. Its session and agent keep running. On the host on 2026-11-04, all four Task sessions still running belonged to done Tasks.
- A done Task makes no Inbox item, so work that goes on in its session never reaches Inbox.
- A Routine run's session also stays after the run publishes. Nothing closes it, and Task lists never show it, so one session and one folder pile up per run.
- muxa for tmux's tree lists every session without a repo under one "sessions" group: manual sessions, repo-less Tasks and Routine runs together. The apps already show only manual sessions in Sessions mode (ADR 0035).

## Decision

1. **A Task that becomes done or dropped closes its session.** This holds for `update-task` (the CLI, tmux's Done and Drop keys, the apps) and for an Inbox answer (Accept, Drop).
   - The host closes the session with the Task's tag as `close-task` does with mode `keep`. Attached clients move to another session first. The worktree or folder, `TASK.md` and the branch stay. Removing a worktree stays a separate action.
   - The close happens after the status is saved. A close that fails is logged; the status stands.
   - Only the change closes. A Task that was done before this decision keeps its session.
   - To keep working in the session, answer Keep open or Later instead of Accept.
2. **A Routine run that published closes its session.** The daemon does it on its tick, for the run named in the Routine's last success, unless it is the open run. Its folder stays until the Routine's next run publishes, so the last good run's `TASK.md` and output can be read; then the daemon removes it. A failed run keeps its session and folder, for inspection.
3. **muxa for tmux's tree shows Task sessions apart from manual sessions.** Sessions with `@muxa_task` list under their repo as Tasks; repo-less ones under "tasks". Routine runs list under "routines" while they run. The "sessions" group holds only manual sessions.

## Consequences

- A finished Task frees its agent process, as parking does (ADR 0034 §4).
- Typing in a done Task's session after Accept is no longer possible. The answer that keeps the session is Keep open.
- A Routine keeps one folder, its last published run's, and no session. Its committed state carries what the next run needs.
- Alternatives not taken:
  - A daemon sweep that closes every done Task's session: it would also close sessions of Tasks done before this decision, where work may still go on.
  - Closing on Accept only: Drop and the CLI would still leave sessions behind.
  - Removing the worktree on done: an unmerged branch or uncommitted change would be lost or block the close.
