#!/usr/bin/env bash
# Agent state per pane, kept in the pane option @agent_state.
# Usage: state.sh set <pane> <running|waiting|idle|done|error|""> [claude session id]
set -u
. "$(dirname "$0")/lib.sh"

usage() {
  echo 'usage: state.sh set <pane> <running|waiting|idle|done|error|""> [session-id]' >&2
  exit 2
}

cmd_set() {
  local pane="$1" state="$2" sid="${3:-}" cur cur_state cur_sid
  case "$state" in running|waiting|idle|done|error|'') ;; *) usage ;; esac

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
  notify_change
}

case "${1:-}" in
  set) shift; [ $# -ge 2 ] || usage; cmd_set "$@" ;;
  *) usage ;;
esac
