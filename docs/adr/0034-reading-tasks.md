# 0034: Reading Tasks: no worktree until the work changes code

## Status

Proposed. The developer chose this shape on 2026-11-04. It extends ADR 0015 (Task targets) and ADR 0031 (ask sessions), and adds parking to the session lifecycle of ADR 0018 and ADR 0024.

## Context

- A repo Task always gets a worktree and a branch (ADR 0007, ADR 0031 §3). Research, review and planning Tasks change no code, but each one still costs a checkout, and often a dependency install and a build cache.
- An ask session (ADR 0031 §4) runs in the main checkout without a worktree. But it has no record, no goal and no Inbox items, so it cannot hold work that lasts.
- A repo-less Task (ADR 0015) has its own directory, but it cannot see the repo's code, its `CLAUDE.md` or its git history.
- Every Task with a session also runs one agent process. Fewer worktrees save disk space. They do not save memory or CPU.

## Decision

### 1. A reading Task works in its own folder and reads the repo

- `create-task` takes `read: true` with a `repo` and no `branch`. The record keeps `repo`, has no `branch`, and has `read: true`.
- The Task gets its own folder, `<workspace root>/tasks/<slug>-<id>/`, the same folder a repo-less Task gets (ADR 0015). The agent starts there, through the built-in launcher. The record's `dir` is this folder.
- The folder holds `TASK.md`, later `HANDOFF.md`, and what the Task produces: a design, a report, a plan. It stays after the Task closes. Its documents and images are the evidence of the Task's `review` item (ADR 0033).
- The repo is a reference. `TASK.md` names the path of its main checkout, tells the agent to read the repo's `AGENTS.md` or `CLAUDE.md` first, to run git with `git -C <path>`, and not to change the repo. It also tells the agent to ask for a branch when a change is needed.
- For Claude Code, the bridge writes `<folder>/.claude/settings.local.json`:
  - `permissions.additionalDirectories` lists the main checkout, so reads there do not ask.
  - `permissions.deny` lists `Edit(//<main checkout>/**)`. An `Edit` rule covers every built-in tool that edits a file, Write included; a `Write(path)` rule never matches. The leading `//` makes the path absolute.
  - Checked against https://code.claude.com/docs/en/permissions and a real run of Claude Code 2.1.318: reads in the checkout work, Write and Edit there are denied, and writes in the folder work.
  - Claude Code reads `settings.local.json` from the git repository root. When the workspace root is inside a git repo, the folder's file is not read, and the Task is read-only only by `TASK.md`.
- Other agents get only `TASK.md`. The bridge does not enforce read-only for them, for the reason in ADR 0031 (Alternatives).
- Several reading Tasks of one repo share its main checkout and never write there. No worktree is made.

### 2. The bridge notices changes it did not expect

- When a reading Task's agent finishes a turn, the bridge compares `git status --porcelain` of the main checkout with the status it read when the turn started.
- New changes turn that turn's item into a `proposal` (ADR 0033): "The agent changed the main checkout". It replaces the turn's `review` item, so a Task keeps one open item. The changed files are in its summary and their count in `attachments.changes`. Clients open evidence through the Task's pane, which is in the folder, so the item has no diff to open. The answers are **Branch** (§3), **Keep** and Later. The bridge never reverts files.

### 3. Branch: a worktree when the work needs one

- `branch-task {task, branch?}` turns a reading Task into a repo Task:
  1. Under the Task lock (ADR 0024), the bridge makes the worktree with the same rules as a new Task, with the branch name from the title (ADR 0031 §3).
  2. It copies `TASK.md` to `<worktree>/.muxa/`. The Task's folder stays, with what it produced. `TASK.md` names the folder, so the agent can read its earlier work.
  3. It writes `HANDOFF.md`, then restarts the agent in the worktree (ADR 0018). The record gets `branch` and `dir`, and loses `read`.
- The conversation is not resumed. Claude Code and Codex store sessions by working directory, so the new agent starts from `TASK.md` and `HANDOFF.md`. That is the existing restart path.
- Changes already in the main checkout stay there. The bridge never moves them, because it cannot tell which changes belong to the Task.

### 4. Park an idle Task

- `park-task {task}` closes the Task's session and keeps its record, `TASK.md` and worktree. `HANDOFF.md` is written first. A parked Task is `open` with no session (ADR 0015).
- Opening a parked Task offers **Resume**: `restart-agent` on an open Task with no session starts a new session in its `dir`, with a restart run and the restart prompt (ADR 0018).
- Park and Resume refuse a Task with a named worker, and a reading Task takes no loop, until those are designed.
- `park_after = "12h"` in a `[[workspace]]` (`m`, `h` or `d`) lets the daemon park a Task whose agent has been idle that long, with no open or later Inbox item, no loop, no prompt wait and no restart. The setting is off by default.
- Every finished turn makes a `review` item. So a Task parks only after the user answered its last item, for example with Keep open. A Task that waits on its review does not park (decided 2026-11-05). The user is likely to reply soon, and a parked Task needs a restart before the reply reaches the agent.
- Parking frees the agent process. Reading Tasks free the checkout. Together they bound what many Tasks cost.

### 5. `work:4`

The bridge advertises `work:4` for `read`, `branch-task`, `park-task` and the answers `branch` and `keep`. Clients that do not know an answer leave it out. A client sends them only to a level 4 host. The daemon serves `branch-task` and `park-task` as record requests, like `restart-agent`.

## Consequences

- A document-only Task, such as a design or a write-up with no repo, is a repo-less Task as before. A reading Task is the same folder with a repo to read.
- The agent's working directory is not the repo. Its code tools need the repo's path. The repo's `CLAUDE.md` is read because `TASK.md` asks for it, not automatically.
- A reading Task reads the main checkout as it is. When the user switches its branch, the Task reads other code. For research this is acceptable. A review of one branch should use `gh pr diff` or `git show`, or Branch.
- Nothing stops the agent from changing the main checkout. §2 makes such a change visible instead.
- Branch costs one restart. The agent loses its conversation and keeps its notes.
- The Changes view of a reading Task shows nothing, because the main checkout's changes are not the Task's. Its output is in its folder.
- Alternatives not taken:
  - A read-only agent mode, such as Claude Code's `--permission-mode plan`: it also blocks writing `TASK.md` and the Task's output, and other agents have no such mode.
  - Running in the main checkout with the files in `<main checkout>/.muxa/tasks/<id>/`: the repo's `CLAUDE.md` and git work as usual, but the output hides in an excluded folder of the repo, and nothing stops the agent from changing the checkout.
  - The `--add-dir` flag instead of the settings file: the bridge would have to change the workspace's agent command, which may be any shell command.
  - A detached worktree per reading Task (`git worktree add --detach`): it still costs a checkout, which is what this ADR saves.
  - Resuming the conversation after Branch: it needs the old working directory, which Branch leaves.
