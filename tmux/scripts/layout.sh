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

# Typed into the pane's shell, so the shell stays when the agent exits.
if [ "$agent" = 1 ]; then
  # The command is a tmux format, expanded for the AI pane.
  t send-keys -t "$ai" -l "$(t display-message -p -t "$ai" "$(opt @agents_launch_cmd "$AGENTS_LAUNCH_CMD")")"
  t send-keys -t "$ai" Enter
fi
t select-pane -t "$ai"
