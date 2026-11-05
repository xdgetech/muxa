# muxa for tmux

muxa's client inside tmux, formerly tmux-agents: a tmux plugin for running many
AI coding sessions at once. A tree on the left shows every task session and its
live agent state. Each task is one session in
its own git worktree, with the AI pane on the left and a terminal on the right.

```
┌─ tree ───────────────┬─ AI session ─────────┬─ terminal ─────────┐
│ ▾ Tasks 3            │                      │                    │
│   📂 web-app         │  claude              │  git, gh, tests    │
│ ▎1 ⚡ add-api         │                      │                    │
│ ▎    ⚡ 0 main        │                      │                    │
│ ▎       1 tests      │                      │                    │
│  2 🙋 fix-ssh #1834  │                      │                    │
│  3 💤 fix-retry ✅   │                      │                    │
│ ▸ Sessions 2 💤      │                      │                    │
│──────────────────────│                      │                    │
│ ⚡ add-api · working │                      │                    │
│ ⎇ feat/add-api       │                      │                    │
│ n new c win o ide x  │                      │                    │
└──────────────────────┴──────────────────────┴────────────────────┘
🙋 needs you  ❌ error  ⚡ working  🏁 finished, not seen yet  💤 idle  ✅ PR merged
```

A finished turn shows 🏁 until you look at its window, then 💤.
The tree has four sections (ADR 0038): **Tasks** (muxa Tasks, by repo),
**Routines** (running Routine runs), **Ask** (ask sessions) and **Sessions**
(every other session, by repo). `Enter` or `space` on a section's heading folds
it in every sidebar. A folded heading shows its count and its most urgent state.
One line per session, labelled `1`–`9` then `a`–`z` for the jump keys; sessions
in a folded section get no label. `▎`
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

## Install

Needs tmux ≥ 3.4, fzf ≥ 0.65 (tested with 0.74), jq, curl and git.

1. Add to `~/.tmux.conf`, then reload it. `<muxa>` is your muxa checkout:

   ```tmux
   run-shell '<muxa>/tmux/agents.tmux'
   ```

2. Report Claude Code state to tmux. This adds one hook command to
   `~/.claude/settings.json` and writes a backup next to it:

   ```sh
   <muxa>/tmux/scripts/install-claude-hooks.sh
   ```

   Undo with `--uninstall`.

   A Claude background job runs outside tmux. Its state goes to the one Claude
   pane in the job's directory. With two such panes, it shows nowhere.
3. Optional, Codex: in `~/.codex/config.toml` set
   `notify = ["<muxa>/tmux/hooks/codex-notify.sh"]`. Codex reports only the end
   of a turn, so a Codex pane shows `○` and nothing else.
4. Optional, Claude skill: lets a Claude session create, list and close tasks,
   also over Remote Control. Link it on the machine that runs tmux:

   ```sh
   ln -s <muxa>/tmux/skills/tmux-agents ~/.claude/skills/tmux-agents
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
| `prefix + T` | the muxa Inbox, in a popup (see [muxa](#muxa)) |
| `prefix + A` | ask about a repo: an agent in its main checkout, with no task (needs muxa) |

In the tree: `j`/`k` move, `Enter` go to the task or fold a section, `space`
fold a section, `1`–`9` go to that task (letters only through `prefix + j`),
`Esc` leave, `n` new task, `c` new window in the task's worktree, `o` IDE,
`r` rename, `R` reload, `?` every key in a popup.
`x` closes the session and removes the worktree, but only when it is clean and
merged. A session with no worktree, such as an ask or manual session, just closes. `X` closes the session and keeps the worktree. `ctrl-x` removes the
worktree even when the branch is not merged, but still not when it has
uncommitted changes. The branch is always kept.
When the session runs a muxa Task, these keys show a menu instead of the y/n
question: close and mark the Task done, close and drop it, or close and keep it
open. The Task changes only once the session is closed.
Every close also stops what is left running in the worktree: processes whose
directory is in it, also orphans, and Docker Compose projects started from it
(`docker compose down`, volumes are kept).

Over SSH, `o` cannot start the IDE on your laptop. It copies
`cursor --remote ssh-remote+<host> <worktree>` to the laptop's clipboard
instead (OSC 52; your terminal must allow it).

## Options

| Option | Default | |
|---|---|---|
| `@agents_launch_cmd` | `claude --name '#{@agent_name}'` | typed into a new AI pane. A tmux format, expanded for that pane. `@agent_name` is `<repo>-<session>`, with `-2`, `-3` for a second agent in the task |
| `@agents_ide_cmd` | `cursor` | |
| `@agents_ide_host` | `hostname -s` | host name in the remote IDE command |
| `@agents_ide_mode` | `auto` | `local`, `remote`, or `auto` (remote when the client came in over ssh/mosh) |
| `@agents_worktrees_root` | `<repo>/../.worktrees` | where new worktrees go |
| `@agents_worktree_cmd` | | command that creates the worktree: gets `<main repo> <branch> <base>`, prints the path last |
| `@agents_sidebar` | `on` | |
| `@agents_sidebar_width` | `28` | |
| `@agents_min_width` | `100` | narrower clients (a phone) get no tree |
| `@agents_folded` | | the folded sections of the tree, space-separated: `tasks routines ask sessions`. `Enter` and `space` set it |
| `@agents_inbox_rows` | `3` | Inbox items the tree shows above the sessions |
| `@agents_inbox_refresh` | `5` | seconds between two `muxa-cli inbox --json` calls for the tree |
| `@agents_split` | `50` | terminal pane width, % of a new task window. Showing, hiding or resizing the tree keeps the current split |
| `@agents_check_gh` | `on` | ask `gh` whether the PR is merged before `x` |
| `@agents_icon_waiting` / `_error` / `_running` / `_idle` / `_done` | `🙋` / `❌` / `⚡` / `💤` / `🏁` | agent states, in the tree and the status bar |
| `@agents_icon_repo` / `_merged` / `_window` / `_branch` | `📂` / `✅` / none / `⎇` | |
| `@agents_footer` | `n new c win o ide x close ? help` | last line of the tree's footer |
| `@agents_key_next` / `_prev` / `_attention` / `_jump` | `)` / `(` / `a` / `j` | prefix keys to switch tasks |
| `@agents_notify` | `on` | desktop notifications |
| `@agents_notify_events` | `waiting done error` | which changes notify; `done` means a finished turn |
| `@agents_notify_cmd` | | command that sends the notification instead, gets `<session> <message>` (e.g. `terminal-notifier`, ntfy) |
| `@agents_key_focus` / `_toggle` / `_new` / `_ide` / `_inbox` / `_ask` | `Tab` / `b` / `N` / `O` / `T` / `A` | |
| `@agents_muxa` | `auto` | `off`: never use muxa, even when it is installed |
| `@agents_muxa_cli` | `muxa-cli` on `PATH` | muxa's CLI on this host |
| `@agents_muxa_bridge` | `muxa-bridge` beside it | the bridge command muxa-cli starts, with its flags |
| `@agents_muxa_workspace` | the first one | the workspace the new-task form and the ask popup suggest when no workspace holds the current repo |

## muxa

With `muxa-cli` and `muxa-bridge` installed on the host, a task is a muxa task record: the same Task its iPad and phone
apps show, with a goal and a status.

- `prefix + N` opens a form in a popup: workspace, repo, branch, title and goal.
  The workspace suggested is the one whose repos hold the current repo.
  The repo is a list to filter: the current repo first, then the workspace's
  repos, by their path under the workspace root (`group/app`), else with `~`.
  `- no repo` starts a task without a repo, in the workspace's own directory.
  A path that is not listed can be typed. An empty branch lets muxa name one
  after the title; an older muxa needs the branch. A title is required.
  muxa starts the task through `new.sh`, hands the goal to the agent in
  `.muxa/TASK.md`, and this client moves to the new session.
- `prefix + A` starts an ask session: muxa's agent in a repo's main checkout,
  to ask about its code. There is no task and no worktree, and the agent can
  change files there. The popup asks for the workspace and picks the repo
  as the form does, runs `muxa-cli ask REPO --workspace W --json`, and moves
  this client to the new session. Without muxa, or with a muxa too old to
  start ask sessions, it shows a message and does nothing.
- `prefix + T` shows the Inbox (ADR 0033): one row per question an agent or a
  Routine has for you, with the selected item below the list: its question, the
  agent's last message, its numbered evidence and its keys.
  - `j`/`k` move. `a` gives the item's main answer: Allow, Complete Task,
    Retry or Approve; a briefing's ✓ Done.
  - `s` Send: the text goes to the agent as it is, and the Task goes on. On a
    permission it is Deny & send: muxa denies the request, then sends the text.
  - `r` Deny, Reject or Redo, `c` Change course, `p` and `n` ✓ Done (closes
    only the item), `x` Drop Task, `l` Later: the item moves below the others
    and stays there until you answer it (ADR 0040). Send, Redo and Change course
    ask for the text.
  - Labels say what the answer does (ADR 0033 §4a): on an agent's permission
    `a` is Allow and `r` Deny; a Routine's proposal keeps Approve and Reject.
  - `1`–`9` open evidence: the diff or a doc in `$PAGER`, an image or a link with
    the system opener (a link is also copied to the tmux buffer). For a question,
    `1`–`9` choose its option instead.
  - `space` and `b` scroll the item a page down and up, `J` and `K` (or
    `Shift`+arrows) a line. `v` shows the whole item in `$PAGER`.
  - `Enter` or `o` goes to the item's session. `ctrl-r` reloads, `q` or `Esc` closes.
  - Only the keys an item takes are listed under it. With a muxa older than
    `work` level 3, `prefix + T` shows the older Task list: `Enter` goes to the
    task's session, `ctrl-d` marks it done and `ctrl-x` drops it.
- The tree starts with the Inbox section: the count and the first open items
  (`@agents_inbox_rows`). `Enter` on a row opens that item in `prefix + T`.
  The section reads a cache that one sidebar refreshes through `muxa-cli inbox
  --json` every `@agents_inbox_refresh` seconds.
- Without muxa-cli, or without a muxa workspace (`~/.config/muxa/workspaces.toml`),
  `prefix + N` makes a plain task as before.

The records stay in muxa. This plugin never writes them; it calls
`muxa-cli ... --json`, documented in `docs/agents.md` ("Desktop tools") and ADR 0017.

## For other tools

Other tools, such as `muxa-bridge`, read and start
tasks through the names below. They stay stable; a change to one is a breaking change.

- `@agents_scripts`: global option, the directory of these scripts. Set on every load.
- `@agents_features`: global option, a space-separated list of what the scripts
  accept beyond this list. `muxa-task`: `new.sh` takes `--muxa-task`.
- `new.sh <repo dir> <branch> [--base REF] [--task ID] [--muxa-task ID] --no-switch` starts a task,
  or finds the running one for the branch. It prints the session name on its last
  line. On failure it exits non-zero. A tool must pass `--no-switch`: without it,
  the attached client moves to the new session.
- `TMUX_AGENTS_SOCKET`: when set, the scripts use `tmux -L <it>`.
- Session options: `@agent_repo` (repo name), `@agent_repo_dir` (main checkout),
  `@agent_branch`, `@agent_worktree`, `@agent_task`, and `@muxa_task`: the muxa
  task record, set by `--muxa-task` when the session is made. On an existing
  session it is set only when the session has none.
- Pane options: `@agents_role` (`ai` or `term`), `@agents_sidebar_pane` (`1` on the
  tree), `@agent_state` (`running`, `waiting`, `idle`, `done`, `error`),
  `@agent_state_at` (epoch seconds), `@agent_session_id`.
- `@muxa_device`: a session with this option set is muxa's device session. It
  holds one window of another session while a phone or tablet shows it. The
  tree, the status line, the keys and the sidebars skip it, and `close.sh`
  kills it with the session whose window it holds.

## Test

```sh
make test-tmux-plugin    # from the muxa root
```

Each test runs its own tmux server, so it never touches your sessions.

## Save and restore repository categories

With tmux-resurrect installed, loading `agents.tmux` registers save/restore
hooks automatically (Python 3.9 or newer required). Use the usual
`prefix + Ctrl-s` to save, and `prefix + Ctrl-r` to restore. Reload the plugin
once after updating, then save once before the next restore.

The snapshot itself includes repository category, main checkout, branch,
worktree and Task links (`@agent_repo`, `@agent_repo_dir`, `@agent_branch`,
`@agent_worktree`, `@agent_task`, `@muxa_task`). Metadata follows the saved
session name to its new tmux id. Existing tags on live sessions win; device
sessions and transient agent state are excluded. No pane layout or process is
changed by metadata recovery. Mixed working directories do not affect grouping.
Existing resurrect hooks still run with their original arguments.

Old snapshots without metadata still restore normally, but cannot recover lost
categories. Repair those tags once and save again. Selecting an older snapshot
also selects its own metadata; there is no separate latest-category file.
If you configure resurrect hooks yourself, do so before loading `agents.tmux`.

For an explicit snapshot (without restoring panes), use:

```sh
python3 scripts/resurrect.py save /path/to/snapshot
python3 scripts/resurrect.py restore /path/to/snapshot
```

`save` appends metadata to an existing resurrect snapshot; `restore` fills
missing identity tags on matching sessions. Saved data is parsed as JSON,
never executed as commands.
