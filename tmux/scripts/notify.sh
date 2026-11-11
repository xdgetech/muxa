#!/usr/bin/env bash
# Sends a desktop notification for an agent state change worth telling about:
# it needs input, it finished a turn, or it failed. Called by state.sh. With
# --result, for an assistant result the Inbox poll saw first (results.sh).
# Usage: notify.sh <pane> <new state> | notify.sh --result <work title>
set -u
. "$(dirname "$0")/lib.sh"

[ "$(opt @agents_notify on)" = on ] || exit 0

# Without @agents_notify_cmd, a result goes through a sidebar pane: muxa's own,
# so no pane of the user changes its options.
if [ "${1:-}" = --result ]; then
  event=result
  title=Inbox
  body="Assistant result: $(printf '%s' "${2:?usage: notify.sh --result <work title>}" | tr -d '\000-\037\177')"
  pane="$(t list-panes -a -f "#{&&:#{@agents_sidebar_pane},$AGENTS_VISIBLE}" -F '#{pane_id}' 2>/dev/null | head -n 1)"
else
  pane="${1:?usage: notify.sh <pane> <new state>}"
  case "${2:-}" in
    waiting) event=waiting body="needs your input" ;;
    error) event=error body="failed" ;;
    done) event="done" body="finished" ;;
    *) exit 0 ;;
  esac
  # No notification for a window you are looking at.
  is_watched "$pane" && exit 0
  title="$(t display-message -p -t "$pane" '#{session_name}' 2>/dev/null)" || exit 0
fi
case " $(opt @agents_notify_events 'waiting done error result') " in *" $event "*) ;; *) exit 0 ;; esac

cmd="$(opt @agents_notify_cmd '')"
if [ -n "$cmd" ]; then
  # shellcheck disable=SC2086  # a command path, optionally with its own flags
  $cmd "$title" "$body" 2>>"$(agents_log_file)" || log "notify: $cmd failed for a $event event"
  exit 0
fi

[ -n "$pane" ] || { log "notify: no pane to send a $event notification through"; exit 0; }
tty="$(t display-message -p -t "$pane" '#{pane_tty}' 2>/dev/null)" || exit 0
# OSC 777 notify, wrapped for tmux passthrough. It goes through the pane;
# "all" lets the pane pass it on while its window is hidden.
[ -w "$tty" ] || { log "notify: cannot write to $tty for a $event event"; exit 0; }
t set-option -p -t "$pane" allow-passthrough all
clean() { printf '%s' "$1" | tr -d '\000-\037;'; }
# shellcheck disable=SC1003  # \\ ends the escape sequence (ST)
printf '\033Ptmux;\033\033]777;notify;%s;%s\a\033\\' "$(clean "$title")" "$(clean "$body")" >"$tty"
