---
name: tmux-agents
description: Create, list, or close tmux-agents tasks (a git worktree plus a tmux session with an AI pane) on this machine. Use when asked to start a new task or branch in its own session, to list the running tasks, or to close one, for example "new task for web-app on feature/foo" or "close the fix-retry task".
---

# tmux-agents

This skill is a symlink into the plugin checkout. Find the scripts from it:

```sh
S="$(cd ~/.claude/skills/tmux-agents && pwd -P)/../../scripts"
```

Set `S` in the same shell command as the script call. Shell variables do not
carry over between commands.

The scripts act on the tmux server of this machine, so run them from a shell inside tmux.

## New task

```sh
"$S/new.sh" <repo dir> <branch> --no-switch [--task ID] [--base REF]
```

- Always pass `--no-switch`. Without it, the user's attached client jumps to the new task.
- `<repo dir>` is any path inside the repo, for example `~/src/web-app`.
  If the user names only the repo, find it under `~/Projects`. Ask when more than one matches.
- `--base` sets the ref a new branch starts from. Omit it to use the repo's default branch.
- On success it prints the session name. Report it, and the agent's name:
  `tmux list-panes -s -t "=<session>" -F '#{@agent_name}' | grep .`. The agent runs with that
  name (`claude --name`), so other sessions can send it messages by that name.
- On failure, the reason is in `${TMPDIR:-/tmp}/tmux-agents-$(id -u)/agents.log`.
  Read the last lines and report them.

## List tasks

```sh
tmux list-sessions -F '#{session_name}	#{@agent_repo}	#{@agent_pr_state}	#{@agent_worktree}' | awk -F'\t' '$4 != ""'
```

## Close a task

```sh
"$S/close.sh" [--keep|--force] <session>
```

- No flag: removes the worktree only when it is clean and the branch is merged.
- `--keep`: closes the session and keeps the worktree.
- `--force`: removes the worktree even when the branch is not merged.
  Never use it unless the user asked for it in this conversation.
- No mode removes a worktree with uncommitted changes, and the branch is always kept.
- Never close the session you are running in. That kills this conversation.
