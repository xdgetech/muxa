# 0068: A Task has one or more places

## Status

Proposed (2026-11-21). Builds on ADR 0015, 0031 and 0034.

## Context

A Task today has one repo and branch, a read-only repo (ADR 0034), or a folder with no repo. Much of the developer's work fits none of these alone:

- A budget or a reply draft has no repo.
- An audit reads several repos and writes notes in a folder.
- A decision compares two or three repos.

## Decision

1. A Task has a list of **places**. Each place is one of:
   - **folder**: a folder muxa creates, named as ADR 0034 names it today (`<slug>-<id>` under the Workspace's tasks folder);
   - **branch**: a worktree of a repo on a new branch, as `tmux/scripts/new.sh` makes it today;
   - **read only**: a repo's main checkout, never written.
2. **The working place is a folder or a branch.** The agent starts there. A Task with only read-only places gets a folder as its working place.
3. muxa writes every place into the Task's `TASK.md` with its path and mode.
4. **Read only is a rule for the agent, not a boundary.** Claude Code gets read-only places as additional directories with edits denied for them. Codex runs with only the working place as a writable root. Other agents get the list in `TASK.md` only.
5. A Task has at most one branch place per repo.
6. A place can be added to an open Task. muxa updates `TASK.md` and tells the agent in one line. A read-only place can become a branch place, as ADR 0034's "Branch" does today.
7. Closing a Task removes its branch worktrees by the rules of ADR 0043. It never touches read-only places. Its folder stays.

## Consequences

- The sidebar shows the places under the Task's name, for example "folder + 3 repos (read)".
- The Reader lists documents from every place.
- Two Tasks may read the same repo. Only one Task writes a given branch.
- The grouping of ADR 0063 (`0063-tasks-group-by-workspace-and-repo.md`) groups by Workspace only; a Task with several repos has no single repo group.
