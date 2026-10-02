#!/usr/bin/env bash
# Splits a task window into AI pane | terminal pane, both in the worktree, and
# starts the agent in the AI pane. The sidebar is added by the tmux hooks.
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

t set-option -p -t "$ai" @agents_role ai
term="$(t split-window -h -d -t "$ai" -c "$wt" -l "$(opt @agents_split 50)%" -P -F '#{pane_id}')" \
  || die "could not split window $win"
t set-option -p -t "$term" @agents_role term

# Typed into the pane's shell, so the shell stays when the agent exits.
if [ "$agent" = 1 ]; then
  t send-keys -t "$ai" -l "$(opt @agents_launch_cmd 'claude --remote-control')"
  t send-keys -t "$ai" Enter
fi
t select-pane -t "$ai"
