#!/usr/bin/env bash
# Splits a task window into AI pane | terminal pane, both in the worktree, and
# starts the agent in the AI pane. The sidebar is added by the tmux hooks.
# The AI pane gets @agent_name, unique on the server, for the launch command.
# Usage: layout.sh <window> [--no-agent]
set -u
. "$(dirname "$0")/lib.sh"

win="${1:?usage: layout.sh <window> [--no-agent]}"
agent=1
[ "${2:-}" = --no-agent ] && agent=0

ai="$(t list-panes -t "$win" -F "#{pane_id}${TAB}#{@agents_sidebar_pane}" | awk -F'\t' '$2 != "1" { print $1; exit }')"
[ -n "$ai" ] || die "window $win has no pane to split"

wt="$(target_opt "$ai" @agent_worktree)"
[ -n "$wt" ] || wt="$(t display-message -p -t "$ai" '#{pane_current_path}')"

# <repo>-<session>, so other sessions can address the agent by a name that says which task it is.
# A second agent for the same task gets -2, -3, ...
agent_name() {
  local base n i=2
  base="$(t display-message -p -t "$ai" '#{?@agent_repo,#{@agent_repo}-,}#{session_name}')"
  n="$base"
  while t list-panes -a -f "$AGENTS_VISIBLE" -F '#{@agent_name}' | grep -qxF "$n"; do
    n="$base-$i"
    i=$((i + 1))
  done
  printf '%s' "$n"
}

t set-option -p -t "$ai" @agents_role ai
t set-option -p -t "$ai" @agent_name "$(agent_name)"
term="$(t split-window -h -d -t "$ai" -c "$wt" -l "$(opt @agents_split 50)%" -P -F '#{pane_id}')" \
  || die "could not split window $win"
t set-option -p -t "$term" @agents_role term

# tmux's own quoting, for a value inside a command that tmux parses later.
tmux_quote() {
  local s="${1//\\/\\\\}"
  s="${s//\"/\\\"}"
  printf '"%s"' "${s//\$/\\\$}"
}

# The agent is the AI pane's program, run by an interactive login shell so it
# gets the user's PATH. Typing it into a starting shell lost it while the
# sidebar resized the pane. When the agent exits, a one-time hook gives the pane
# a login shell, as the bridge's built-in launcher does.
start_agent() {
  local cmd sh
  # The command is a tmux format, expanded for the AI pane.
  cmd="$(t display-message -p -t "$ai" "$(opt @agents_launch_cmd "$AGENTS_LAUNCH_CMD")")"
  sh="$(t show-options -gqv default-shell)"
  [ -n "$sh" ] || sh=/bin/sh
  t set-hook -p -t "$ai" pane-died "set-hook -pu -t $ai pane-died ; set-option -pu -t $ai remain-on-exit ; respawn-pane -k -t $ai -c $(tmux_quote "$wt") -- $(tmux_quote "$sh") -l"
  t set-option -p -t "$ai" remain-on-exit on
  # new.sh gave the first shell these; a respawn would drop them.
  t respawn-pane -k -t "$ai" -c "$wt" \
    -e "AGENTS_REPO_DIR=$(target_opt "$ai" @agent_repo_dir)" -e "AGENTS_WORKTREE=$wt" \
    -e "AGENTS_BRANCH=$(target_opt "$ai" @agent_branch)" -e "AGENTS_TASK=$(target_opt "$ai" @agent_task)" \
    -- "$sh" -lic "$cmd" || die "could not start the agent in $ai"
}

[ "$agent" = 1 ] && start_agent
t select-pane -t "$ai"
