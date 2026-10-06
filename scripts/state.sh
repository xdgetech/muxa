#!/usr/bin/env bash
# Agent state per pane, kept in the pane options @agent_state and @agent_state_at.
# "done" is a finished turn you have not looked at; it becomes idle once you do.
# Usage: state.sh set <pane> <running|waiting|idle|done|error|""> [claude session id]
#        state.sh seen   (done → idle in every window someone is looking at)
set -u
. "$(dirname "$0")/lib.sh"

usage() {
  echo 'usage: state.sh set <pane> <running|waiting|idle|done|error|""> [session-id] | seen' >&2
  exit 2
}

cmd_set() {
  local pane="$1" state="$2" sid="${3:-}" cur cur_state cur_sid
  case "$state" in running|waiting|idle|done|error|'') ;; *) usage ;; esac
  # Finished while you watch: you have seen it.
  [ "$state" = done ] && is_watched "$pane" && state=idle

  # A pane that no longer exists is not an error: the agent outlived it.
  cur="$(t display-message -p -t "$pane" "#{@agent_state}${TAB}#{@agent_session_id}" 2>/dev/null)" || return 0
  cur_state="${cur%%"$TAB"*}"
  cur_sid="${cur#*"$TAB"}"

  if [ -n "$sid" ] && [ "$sid" != "$cur_sid" ]; then
    t set-option -p -t "$pane" @agent_session_id "$sid"
  fi
  [ "$state" = "$cur_state" ] && return 0

  if [ -z "$state" ]; then
    t set-option -pu -t "$pane" @agent_state
  else
    t set-option -p -t "$pane" @agent_state "$state"
  fi
  # Epoch seconds, for the "4m" age in the sidebar details.
  t set-option -p -t "$pane" @agent_state_at "$(date +%s)"
  "$AGENTS_SCRIPTS/notify.sh" "$pane" "$state" </dev/null >/dev/null 2>&1 &
  notify_change
}

cmd_seen() {
  local w pane st
  for w in $(watched_windows); do
    t list-panes -t "$w" -F "#{pane_id}${TAB}#{@agent_state}" 2>/dev/null |
      while IFS="$TAB" read -r pane st; do
        [ "$st" = done ] && cmd_set "$pane" idle
      done
  done
  return 0
}

case "${1:-}" in
  set) shift; [ $# -ge 2 ] || usage; cmd_set "$@" ;;
  seen) cmd_seen ;;
  *) usage ;;
esac
