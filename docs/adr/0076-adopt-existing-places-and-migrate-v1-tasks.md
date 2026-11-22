# 0076: Adopt existing places, and migrate v1 Tasks as Inbox items

## Status

Proposed (2026-11-22). Builds on ADR 0043, 0066, 0068 and 0071. Wire: `protocol/PROTOCOL-v2.md`, `start-task` and Migration from v1.

## Context

Since F5 (ADR 0071) the daemon ignores the Task records of protocol v1 (`<state dir>/t-<id>.json`). The developer's host still has open v1 Tasks whose folders and worktrees hold work and, for Claude Code, a conversation. A v2 `start-task` can only make a new folder or find a branch's worktree, so it cannot start an agent in such a folder.

## Decision

1. **`start-task` takes a place kind `existing {path}`.** It adopts a folder or git worktree that is there already. Nothing is made.
   - The path is resolved with its symlinks. It must be inside the Workspace's root, or a linked worktree of one of the Workspace's repos. Anything else is refused.
   - A linked worktree on a branch becomes a `branch` place with that branch, taken from git. Approve then closes it by ADR 0043's rules, as for a worktree muxa made in v2. Any other path is a `folder` place, which closing never removes.
   - It is the working place. It cannot come with a folder or a branch place; read-only places may be added.
   - `.muxa/TASK.md` is written only when it is missing, so the old notes stay. `.muxa/` goes into the repo's `info/exclude`.
2. **Two agents never share a folder.** When a `claude` or `codex` process already works in the folder (the process list and each process's working directory, `src/agents/`), the start is refused with `conflict`. When the agent there is muxa's own (its `muxa.dir` is the path), the reply returns it with `existing: true`.
3. **Claude Code continues its conversation.** The agent gets `--continue` only when the Workspace's Claude Code config dir (its `CLAUDE_CONFIG_DIR`, else the daemon's) holds a conversation for the folder under `projects/`. muxa never guesses one.
4. **Migration adds Inbox items; it starts no agent.** `migrate-v1` reads the open v1 records (`open` or `review`). For each whose place and Workspace still exist, it adds one To do item "Resume: <title>" with a new field `place {workspace, path}` and a note that it came from the old muxa. The item's key `v1:<id>` is `once`, so a second run adds nothing, also after the item is done. The apps offer Resume Task on such an item: `start-task` with the `existing` place, then Done.
5. **Migration is read only.** It never changes or deletes a v1 record, a worktree or a branch. Records whose place is gone are reported and not added.

## Consequences

- The developer moves each old Task into v2 when they want to work on it, in their own order. The 24 open records do not become 24 agents at once.
- The v1 files stay. A second run, or a run after the developer cleaned up by hand, is safe.
- ADR 0043's worktree list files (`<state dir>/../worktrees/`) are not read. An adopted worktree is closed like any branch place on the agent's labels.
- An adopted folder is not new each time, unlike a `folder` place, so a second Resume finds the running agent.
- A restarted adopted agent (Retry) does not get `--continue` (ADR 0072 §5).
- Alternatives not taken:
  - Start every open v1 Task's agent during migration: many agents at once, most of them not wanted now, and each one costs usage.
  - Port the v1 record store: ADR 0071 removed it, and a Task in v2 is its agent terminal and its labels.
  - Let a client name any path: the Workspace boundary is the rule of invariant 3.
