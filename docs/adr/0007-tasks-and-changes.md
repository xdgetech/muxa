# 0007: Starting tasks, and reading uncommitted changes, from a client

## Status

Accepted.

## Context

From the iPad, the user wants to start new work the way they do on the desktop (a git worktree on a branch, and a session with an agent in it), and to read what an agent changed before committing it. Until now muxa only read and typed into panes that already existed. The author's desktop uses tmux-agents, but muxa must work the same without it.

## Decision

- **A task launcher starts tasks, behind `new-task`.** The bridge picks one per request:
  1. `--task-launcher` when given;
  2. tmux-agents' `new.sh`, when tmux-agents publishes its scripts directory in the global option `@agents_scripts`;
  3. otherwise the built-in launcher.
- **tmux-agents' `new.sh` runs as is**, with `--no-switch` always. Without the flag, the desktop client would jump to the new session. The bridge passes `TMUX_AGENTS_SOCKET` for `--socket`, and puts the tmux it found first on `PATH`. The names muxa relies on are listed in tmux-agents' README under "For other tools".
- **The built-in launcher follows tmux-agents' rules:**
  - The worktree goes to `<repos dir>/.worktrees/<repo>-<branch with / as ->`.
  - An existing local branch is checked out, a remote-only branch is tracked, and a new branch starts from the base (default `origin/HEAD`).
  - The session name is the branch with `.`, `:`, `/` and space as `-`.
  - It starts a detached session running the agent command (`--task-command`, default `claude`) in one pane tagged `@muxa_role agent`.
  - It makes no layout and no sidebar; those belong to tmux-agents.
- **A new session never touches an existing one.** It is created with `new-session -d`. No client is switched, and no existing window or pane changes (invariant 2 holds). A branch that already has a session gets that session.
- **Without tags, the bridge fills in what tags would say.** A session without `@agent_repo_dir` gets its repo from the git repo of its first visible pane. In a window with no agent pane, a pane where an agent adapter sees its agent (`AgentAdapter::claims`) becomes the agent, and the window's other untagged panes become shells. Tags always win.
- **Uncommitted changes are listed and followed like documents (ADR 0006).**
  - The bridge lists what differs from HEAD (`git status`, untracked files included).
  - A `diff` must name a listed path.
  - The bridge sends the whole unified diff, and again when it changes (every 2 s). A diff is capped at 2 MiB.
  - git runs with `GIT_OPTIONAL_LOCKS=0`, so it never contends with the agent's own git.
  - The Reader renders the diff with plain DOM text in its offline web view.

## Consequences

- Starting a task writes to the host: a worktree, and maybe a fetch. Tests use fake scripts and throwaway `-L` servers only.
- tmux-agents must be reloaded once (`prefix + I`, or sourcing its `agents.tmux`) before `@agents_scripts` exists. Until then the built-in launcher runs, and the task has no tmux-agents layout.
- When `new.sh` fails, tmux-agents tells the attached tmux client. stderr can then be empty, and the client gets a generic message.
- The built-in launcher copies tmux-agents' worktree and naming rules. A change there must be copied here. Unit tests pin both rules.
- Rejected: the bridge always making worktrees and sessions itself. With tmux-agents installed, the bridge would duplicate its layout, sidebar and state tagging, and the two would drift.
- Rejected: taking tmux-agents into this repo. It is a desktop plugin that changes layouts, and muxa's first promise is not to. Both are useful without the other.
