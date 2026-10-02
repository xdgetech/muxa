# tmux-agents

A tmux plugin for running many AI coding sessions at once. A tree on the left
shows every task session and its live agent state. Each task is one session in
its own git worktree, with the AI pane on the left and a terminal on the right.

```
┌─ tree ───────────────┬─ AI session ─────────┬─ terminal ─────────┐
│  lisk-sdk           │                      │                    │
│ ├ ● AGT-123          │  claude              │  git, gh, tests    │
│ └ ◐ AGT-130 #1834    │                      │                    │
└──────────────────────┴──────────────────────┴────────────────────┘
● running  ◐ waiting for you  ○ idle  ✓ PR merged  ✕ error
```

The design and the remaining phases are in [PLAN.md](PLAN.md).

## Install

Needs tmux ≥ 3.3, fzf ≥ 0.65 (tested with 0.74), jq, curl, git, and a Nerd Font for the icons.

1. Add to `~/.tmux.conf`, then press `prefix + I`:

   ```tmux
   set -g @plugin 'xdgetech/tmux-agents'
   ```

   The repo is private, so TPM clones it with your git credentials.
2. Report Claude Code state to tmux. This adds one hook command to
   `~/.claude/settings.json` and writes a backup next to it:

   ```sh
   ~/.tmux/plugins/tmux-agents/scripts/install-claude-hooks.sh
   ```

   Undo with `--uninstall`.
3. Optional, Codex: in `~/.codex/config.toml` set
   `notify = ["<plugin dir>/hooks/codex-notify.sh"]`. Codex reports only the end
   of a turn, so a Codex pane shows `○` and nothing else.

## Keys

| Key | Does |
|---|---|
| `prefix + Tab` | into the tree, and back out |
| `prefix + b` | show or hide the tree |
| `prefix + N` | new task: asks for repo and branch |
| `prefix + O` | open the current task in the IDE |

In the tree: `j`/`k` move, `Enter` go to the task, `Esc` leave, `n` new task,
`c` new window in the task's worktree, `o` IDE, `r` rename, `R` reload.
`x` closes the session and removes the worktree, but only when it is clean and
merged. `X` closes the session and keeps the worktree. The branch is always kept.

Over SSH, `o` cannot start the IDE on your laptop. It copies
`cursor --remote ssh-remote+<host> <worktree>` to the laptop's clipboard
instead (OSC 52; your terminal must allow it).

## Options

| Option | Default | |
|---|---|---|
| `@agents_launch_cmd` | `claude --remote-control` | typed into a new AI pane |
| `@agents_ide_cmd` | `cursor` | |
| `@agents_ide_host` | `hostname -s` | host name in the remote IDE command |
| `@agents_ide_mode` | `auto` | `local`, `remote`, or `auto` (remote when the client came in over ssh/mosh) |
| `@agents_worktrees_root` | `<repo>/../.worktrees` | where new worktrees go |
| `@agents_worktree_cmd` | | command that creates the worktree: gets `<main repo> <branch> <base>`, prints the path last |
| `@agents_sidebar` | `on` | |
| `@agents_sidebar_width` | `28` | |
| `@agents_min_width` | `100` | narrower clients (a phone) get no tree |
| `@agents_split` | `50` | terminal pane width, % |
| `@agents_check_gh` | `on` | ask `gh` whether the PR is merged before `x` |
| `@agents_key_focus` / `_toggle` / `_new` / `_ide` | `Tab` / `b` / `N` / `O` | |

## Test

```sh
bats tests
```

Each test runs its own tmux server, so it never touches your sessions.
