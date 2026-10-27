# tmux-agents

A tmux plugin for running many AI coding sessions at once. A tree on the left
shows every task session and its live agent state. Each task is one session in
its own git worktree, with the AI pane on the left and a terminal on the right.

```
┌─ tree ───────────────┬─ AI session ─────────┬─ terminal ─────────┐
│ 📂 lisk-sdk          │                      │                    │
│ ▎1 ⚡ AGT-123         │  claude              │  git, gh, tests    │
│ ▎    ⚡ 0 main        │                      │                    │
│ ▎       1 tests      │                      │                    │
│  2 🙋 AGT-130 #1834  │                      │                    │
│  3 💤 fix-retry ✅   │                      │                    │
│──────────────────────│                      │                    │
│ ⚡ AGT-123 · working │                      │                    │
│ ⎇ feat/login         │                      │                    │
│ 📁 lisk-sdk-feat-log │                      │                    │
│ n new c win o ide x  │                      │                    │
└──────────────────────┴──────────────────────┴────────────────────┘
🙋 needs you  ❌ error  ⚡ working  🏁 finished, not seen yet  💤 idle  ✅ PR merged
```

A finished turn shows 🏁 until you look at its window, then 💤.
One line per session, labelled `1`–`9` then `a`–`z` for the jump keys. `▎`
marks the session you are in, and only that session lists its windows. The
footer shows the row under the cursor: its state and for how long, its branch
(read live from git), its worktree, PR and window count.

## Notifications

You get a desktop notification when an agent needs your input, finishes a
turn, or fails, but not for the pane you are looking at. The plugin sends OSC
777 through tmux passthrough, so it works over SSH in terminals that show it
(Ghostty, WezTerm, kitty). It sets `allow-passthrough all` only on panes that
notify. Claude Code can also notify on its own: if you get two, set its
`preferredNotifChannel` to `notifications_disabled`, or `@agents_notify` to `off`.
Skipping the pane you look at needs `set -g focus-events on`.

The design and the remaining phases are in [PLAN.md](PLAN.md).

## Install

Needs tmux ≥ 3.4, fzf ≥ 0.65 (tested with 0.74), jq, curl and git.

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

   A Claude background job runs outside tmux. Its state goes to the one Claude
   pane in the job's directory. With two such panes, it shows nowhere.
3. Optional, Codex: in `~/.codex/config.toml` set
   `notify = ["<plugin dir>/hooks/codex-notify.sh"]`. Codex reports only the end
   of a turn, so a Codex pane shows `○` and nothing else.
4. Optional, Claude skill: lets a Claude session create, list and close tasks,
   also over Remote Control. Link it on the machine that runs tmux:

   ```sh
   ln -s ~/.tmux/plugins/tmux-agents/skills/tmux-agents ~/.claude/skills/tmux-agents
   ```

## Keys

| Key | Does |
|---|---|
| `prefix + Tab` | into the tree, and back out |
| `prefix + b` | show or hide the tree |
| `prefix + N` | new task: asks for repo and branch |
| `prefix + O` | open the current task in the IDE |
| `prefix + )` / `prefix + (` | next / previous task, in tree order |
| `prefix + j`, then `1`–`9` or `a`–`z` | the task with that label in the tree |
| `prefix + a` | the task that needs you most: 🙋, then ❌, then 🏁 |

In the tree: `j`/`k` move, `Enter` go to the task, `1`–`9` go to that task
(letters only through `prefix + j`), `Esc` leave, `n` new task, `c` new window
in the task's worktree, `o` IDE, `r` rename, `R` reload.
`x` closes the session and removes the worktree, but only when it is clean and
merged. `X` closes the session and keeps the worktree. `ctrl-x` removes the
worktree even when the branch is not merged, but still not when it has
uncommitted changes. The branch is always kept.
Every close also stops what is left running in the worktree: processes whose
directory is in it, also orphans, and Docker Compose projects started from it
(`docker compose down`, volumes are kept).

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
| `@agents_split` | `50` | terminal pane width, % of a new task window. Showing, hiding or resizing the tree keeps the current split |
| `@agents_check_gh` | `on` | ask `gh` whether the PR is merged before `x` |
| `@agents_icon_waiting` / `_error` / `_running` / `_idle` / `_done` | `🙋` / `❌` / `⚡` / `💤` / `🏁` | agent states, in the tree and the status bar |
| `@agents_icon_repo` / `_merged` / `_window` / `_branch` | `📂` / `✅` / none / `⎇` | |
| `@agents_footer` | `n new c win o ide x close` | last line of the tree's footer |
| `@agents_key_next` / `_prev` / `_attention` / `_jump` | `)` / `(` / `a` / `j` | prefix keys to switch tasks |
| `@agents_notify` | `on` | desktop notifications |
| `@agents_notify_events` | `waiting done error` | which changes notify; `done` means a finished turn |
| `@agents_notify_cmd` | | command that sends the notification instead, gets `<session> <message>` (e.g. `terminal-notifier`, ntfy) |
| `@agents_key_focus` / `_toggle` / `_new` / `_ide` | `Tab` / `b` / `N` / `O` | |

## For other tools

Other tools, such as [muxa](https://github.com/xdgetech/muxa), read and start
tasks through the names below. They stay stable; a change to one is a breaking change.

- `@agents_scripts`: global option, the directory of these scripts. Set on every load.
- `new.sh <repo dir> <branch> [--base REF] [--task ID] --no-switch` starts a task,
  or finds the running one for the branch. It prints the session name on its last
  line. On failure it exits non-zero. A tool must pass `--no-switch`: without it,
  the attached client moves to the new session.
- `TMUX_AGENTS_SOCKET`: when set, the scripts use `tmux -L <it>`.
- Session options: `@agent_repo` (repo name), `@agent_repo_dir` (main checkout),
  `@agent_branch`, `@agent_worktree`, `@agent_task`.
- Pane options: `@agents_role` (`ai` or `term`), `@agents_sidebar_pane` (`1` on the
  tree), `@agent_state` (`running`, `waiting`, `idle`, `done`, `error`),
  `@agent_state_at` (epoch seconds), `@agent_session_id`.

## Test

```sh
bats tests
```

Each test runs its own tmux server, so it never touches your sessions.
