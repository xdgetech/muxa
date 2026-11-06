# 0043: A worktree goes when nothing holds it

## Status

Proposed. The developer chose this on 2026-11-06. It supersedes the part of ADR 0037 §1 that keeps the worktree when a Task ends. It changes muxa for tmux's close (ADR 0017, 0032) and raises the `work` level as ADR 0027 §5 says.

## Context

- Done and Drop close a Task's session but keep its worktree, folder and branch (ADR 0037 §1). Nothing removes them later, so one worktree piles up per Task.
- muxa for tmux's close removes the worktree when it is clean and merged (`tmux/scripts/close.sh`). Its "Close, mark the Task done" removes it; the apps' Done keeps it. The same end gives two results.
- `X` in tmux closes a session and keeps its worktree. Without a Task nothing refers to that worktree again.
- A session that ends outside muxa (killed by hand, the tmux server restarts) leaves its worktree with no owner.
- No path deletes a Task's local branch (`close.sh` keeps it always).

## Decision

1. **A worktree stays while an open Task or a live session refers to it.** A parked Task is open, so its worktree stays (ADR 0034 §4). When neither refers to it, muxa removes the worktree and its local branch.
2. **Before a session closes, muxa checks whether anything can be lost.**
   - Clean and merged: the session closes, its processes stop (`stop-worktree.sh`), then the worktree and the branch (`git branch -d`) go. No question.
   - Uncommitted changes, or not merged: muxa asks, and says which. Yes removes as above, with `git branch -D`. No changes nothing: the session stays, and the Task keeps its status.
   - Merged means what `close.sh` says today: the PR is merged (`@agent_pr_state` or `gh`), or `HEAD` is in the base branch.
3. **Every end goes through 2.**

   | End | Worktree |
   |---|---|
   | Done or Drop: `update-task`, an Inbox answer, tmux's Done and Drop | 2 |
   | Close of a session without a Task | 2 |
   | Park, and tmux's "Close, keep the Task open" | stays |
   | Close of an ask session (ADR 0039) | none to remove; the main checkout stays |

   `X` (close, keep the worktree) parks the Task when the session has one. A session without a Task no longer offers it.
4. **A Task's end checks before it saves.** With `work` level 7, `update-task` to `done` or `dropped`, and an `update-inbox` answer that ends the Task, take `worktree: "remove" | "force"`.
   - `remove`: when 2 would ask, the request fails with the new error code `would-lose-work`, and its message says why. Nothing is saved and nothing closes. The client asks, then sends `force`.
   - The host does it under the Task's claim: check, close the session, remove the worktree and branch, then save the status. A parked Task's worktree goes the same way.
   - Without `worktree` the host acts as ADR 0037 says: the worktree stays. Older clients keep working.
   - `muxa-cli task done|drop` and `inbox answer` send `remove`, and `force` with `--force`. muxa for tmux asks y/n on `would-lose-work`, then runs them again with `--force`.
5. **muxa keeps a list of the worktrees it created.** `new.sh` and the built-in launcher write one file per worktree under `$XDG_STATE_HOME/muxa/worktrees/`, so the bridge and the plugin never edit the same file. Only listed worktrees are ever removed. Claude Code's `.claude/worktrees` and worktrees made by hand are not listed.
6. **A sweep catches ends that nobody answered.** The daemon sweeps when it starts, which is at login, and then every 10 minutes, in the background of its tick. It reads the list. A listed worktree is held by an open Task, by a tmux session's worktree, or by any tmux pane or process whose directory is in it. One that nothing holds:
   - clean and merged: removed as in 2;
   - otherwise: it stays, listed, and the daemon logs it with the reason.
   - A worktree removed by hand is taken off the list.

   Asking the user about the second kind (a row in tmux's tree, an Inbox item) is a follow-up. It needs a new Inbox item kind in the apps.
7. **One rule, two copies.** `close.sh` stays the rule for muxa for tmux, which must work without the apps. The built-in launcher copies it, and the contract test (ADR 0017) checks that both give the same result. A removal of a Task's worktree holds the Task's claim (ADR 0024), as close, park and restart do.
8. **Existing worktrees are collected once.** The daemon's first sweep after this change lists the `dir` of every Task record that is a worktree, then handles them as in 6. Worktrees without a record are not listed: muxa cannot tell who made them.

## Consequences

- A finished Task leaves nothing on disk, unless the developer chose to keep it.
- Done or Drop on a Task with unmerged work takes one more answer.
- The list in 5 is the only source of "muxa made this worktree". A worktree created before this change without a Task record is never touched.
- The sweep runs in the daemon, not when tmux starts. It costs tmux nothing, and it also catches a session that ended while tmux kept running. A host without the daemon removes worktrees only when a session or Task ends through muxa.
- Alternatives not taken:
  - Save a patch, untracked files and a git bundle, then remove without asking: more code, a store that grows, and data the developer would have to find to restore. Asking costs one answer and loses nothing.
  - Find orphans by walking the projects folder or `git worktree list` alone: slow, and it reaches worktrees that other tools own.
  - Keep cleanup in host scripts (`mise` tasks): the rule would live outside muxa, and a host without them would pile up worktrees again.
  - Place `.worktrees/` at the workspace root instead of beside the repo: a separate change to `new.sh`'s naming contract, not needed for cleanup.
