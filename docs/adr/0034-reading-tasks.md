# 0034: Reading Tasks: no worktree until the work changes code

## Status

Proposed. The developer chose this shape on 2026-11-04. It extends ADR 0015 (Task targets) and ADR 0031 (ask sessions), and adds parking to the session lifecycle of ADR 0018 and ADR 0024.

## Context

- A repo Task always gets a worktree and a branch (ADR 0007, ADR 0031 §3). Research, review and planning Tasks change no code, but each one still costs a checkout, and often a dependency install and a build cache.
- An ask session (ADR 0031 §4) runs in the main checkout without a worktree. But it has no record, no goal and no Inbox items, so it cannot hold work that lasts.
- A repo-less Task (ADR 0015) has its own directory, but it cannot see the repo's code, its `CLAUDE.md` or its git history.
- Every Task with a session also runs one agent process. Fewer worktrees save disk space. They do not save memory or CPU.

## Decision

### 1. A reading Task runs in the main checkout

- `create-task` takes `read: true` with a `repo` and no `branch`. The record keeps `repo`, has no `branch`, and has `read: true`.
- The session starts in the repo's **main checkout**, on whatever branch it has, like an ask session. It is tagged `@muxa_task` as any Task, and always uses the built-in launcher.
- Its files live in `<main checkout>/.muxa/tasks/<task id>/`: `TASK.md`, and later `HANDOFF.md`. `.muxa/` is already in git's exclude file (ADR 0015). So several reading Tasks share one checkout without conflicts, and the agent writes only inside its own folder.
- The record's `dir` is the main checkout. Sessions are linked by `@muxa_task`. The fallback by directory (ADR 0015) skips reading Tasks, because several of them share the directory.
- `TASK.md` tells the agent that this Task reads and must not change the repo, and that it should ask for a branch when a change is needed. The bridge does not enforce this, for the reason in ADR 0031 (Alternatives).

### 2. The bridge notices changes it did not expect

- When a reading Task's agent finishes a turn, the bridge compares `git status --porcelain` of the main checkout with the status it read when the turn started.
- New changes make a `proposal` Inbox item (ADR 0033): "changed the main checkout". The changed files are its attachment. The answers are **Branch** (§3) and **Keep**. The bridge never reverts files.

### 3. Branch: a worktree when the work needs one

- `branch-task {task, branch?}` turns a reading Task into a repo Task:
  1. Under the Task lock (ADR 0024), the bridge makes the worktree with the same rules as a new Task, with the branch name from the title (ADR 0031 §3).
  2. It moves `.muxa/tasks/<id>/` to `<worktree>/.muxa/`.
  3. It writes `HANDOFF.md`, then restarts the agent in the worktree (ADR 0018). The record gets `branch` and `dir`, and loses `read`.
- The conversation is not resumed. Claude Code and Codex store sessions by working directory, so the new agent starts from `TASK.md` and `HANDOFF.md`. That is the existing restart path.
- Changes already in the main checkout stay there. The bridge never moves them, because it cannot tell which changes belong to the Task.

### 4. Park an idle Task

- `park-task {task}` closes the Task's session and keeps its record, `TASK.md` and worktree. `HANDOFF.md` is written first. A parked Task is `open` with no session (ADR 0015).
- Opening a parked Task offers **Resume**, which is the restart path of ADR 0018 in the Task's `dir`.
- `[workspace] park_after = "12h"` lets the daemon park a Task whose agent has been idle that long, with no open Inbox item. The setting is off by default.
- Parking frees the agent process. Reading Tasks free the checkout. Together they bound what many Tasks cost.

### 5. `work:4`

The bridge advertises `work:4` for `read`, `branch-task` and `park-task`. A client sends them only to a level 4 host. The daemon serves `branch-task` and `park-task` as record requests, like `restart-agent`.

## Consequences

- A reading Task reads the main checkout as it is. When the user switches its branch, the Task reads other code. For research this is acceptable. A review of one branch should use `gh pr diff` or `git show`, or Branch.
- Nothing stops the agent from changing the main checkout. §2 makes such a change visible instead.
- Branch costs one restart. The agent loses its conversation and keeps its notes.
- The Changes view of a reading Task shows nothing, because the main checkout's changes are not the Task's.
- Alternatives not taken:
  - A read-only agent mode, such as Claude Code's `--permission-mode plan`: it also blocks writing `TASK.md`, and other agents have no such mode.
  - Running in the Task directory with the repo added as an extra directory (`--add-dir`): the agent then misses the repo's `CLAUDE.md` and runs git outside the repo. The flag is also agent-specific.
  - A detached worktree per reading Task (`git worktree add --detach`): it still costs a checkout, which is what this ADR saves.
  - Resuming the conversation after Branch: it needs the old working directory, which Branch leaves.
