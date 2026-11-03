# 0031: Workspaces are Entities: repos under the root, optional branch, ask sessions

## Status

Proposed. Extends ADR 0015: it does the "limiting repos to workspace roots" that ADR 0015 deferred, without making `repos` outside the root invalid.

## Context

- A workspace is an Entity (`docs/vision.md`, Concepts): its own root folder, accounts, MCP and memory. The developer keeps each Entity's repos in one folder, some one level deeper (`fairpull/fairpull-plan`), with tmux-agents' `.worktrees/` beside them.
- Until now `root` only held repo-less Task directories. Every repo had to be an absolute path in `repos`, and a repo under the root but not listed belonged to no workspace.
- Starting a repo Task needed a branch name. On a phone, inventing one is friction, and the title already says what the work is.
- "What does this code do?" needs no Task, no worktree and no record. It only needs the workspace's agent, with its accounts, in the repo.

## Decision

1. **Repos under the root.** The bridge finds the git repos under each workspace root: a folder with a `.git` directory (not a worktree's `.git` file), one or two levels down. It skips dot-folders, `tasks/`, `routines/`, symlinks and the inside of a repo. A scan reads at most 2000 entries and runs at most 250 ms (`crates/muxa-bridge/src/records/repos.rs`).
   - `task-list` sends them merged with `repos`, absolute and sorted, and the new `root`. `repos` stays valid for repos outside the root.
   - A repo under a workspace's root belongs to that workspace, like a listed one: a Task there from another workspace is refused (ADR 0016). This check reads the path; it does not scan.
2. **Relative repo names.** `create-task` and `start-session` take `repo` relative to the workspace root, e.g. `cto-copilot` or `fairpull/fairpull-plan`. The bridge canonicalizes it and requires it to stay inside the root, so a symlink cannot leave it, unless it reaches a listed repo. A plain name may also name a listed repo outside the root by its folder name. An absolute path works as before.
3. **Branch from the title.** A repo Task without a branch gets the title as a lowercase ASCII slug: `-` between words, at most 40 characters, no leading or trailing `-`, `task` when nothing is left. When that local or `origin` branch, its worktree folder or the main checkout's current branch exists, the bridge adds `-2`, `-3`, and so on. The base stays the repo's default branch (`origin/HEAD`). The main checkout's current branch is never the worktree's branch.
4. **Ask sessions.** `start-session {workspace, repo}` starts a detached session `ask-<repo>-<n>` in the repo's **main checkout**, on whatever branch it has. It runs the workspace's agent command and environment (ADR 0016), as a Task would. It has `@muxa_ask` (the workspace name) and no `@muxa_task` or `@muxa_device`, so it lists under Sessions as a manual session. No record is written. Closing it is closing any manual session.
5. **`work:2`.** The bridge advertises `work` level 2 for all three. A client sends a relative `repo`, omits `branch` or sends `start-session` only to a level 2 host. The host daemon does not serve `start-session` and still says level 1.

## Consequences

- An ask session's agent can change files in the main checkout and run anything the workspace's agent may run. Nothing enforces read-only. The name and the Sessions list make clear where it runs.
- A repo under a root no longer accepts Tasks from another workspace. A repo that two workspaces need belongs in both `repos` lists, outside both roots.
- `task-list` costs a scan of each root per request. The limits keep it bounded; a scan that hits them logs a warning and lists what it found.
- Alternatives not taken:
  - A `[workspace] repos = "auto"` switch: one more setting for what the root already says.
  - A deeper or unbounded scan: Entity folders are shallow, and a home folder as root must stay cheap.
  - A read-only ask agent: no agent here offers one that the bridge could rely on.
  - Asking in a fresh worktree: it costs a branch and a checkout for a question, and it is a Task in all but name.
