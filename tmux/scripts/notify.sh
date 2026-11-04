#!/usr/bin/env bash
# Sends a desktop notification for an agent state change worth telling about:
# it needs input, it finished a turn, or it failed. Called by state.sh.
# Usage: notify.sh <pane> <new state>
set -u
. "$(dirname "$0")/lib.sh"

pane="${1:?usage: notify.sh <pane> <new state>}"
new="${2:-}"

[ "$(opt @agents_notify on)" = on ] || exit 0
case "$new" in
  waiting) event=waiting body="needs your input" ;;
  error) event=error body="failed" ;;
  done) event="done" body="finished" ;;
  *) exit 0 ;;
esac
case " $(opt @agents_notify_events 'waiting done error') " in *" $event "*) ;; *) exit 0 ;; esac

# No notification for a window you are looking at.
is_watched "$pane" && exit 0
info="$(t display-message -p -t "$pane" "#{session_name}${TAB}#{pane_tty}" 2>/dev/null)" || exit 0
IFS="$TAB" read -r session tty <<EOF
$info
EOF

cmd="$(opt @agents_notify_cmd '')"
if [ -n "$cmd" ]; then
  # shellcheck disable=SC2086  # a command path, optionally with its own flags
  $cmd "$session" "$body" 2>>"$(agents_log_file)" || log "notify: $cmd failed for $session"
  exit 0
fi

# OSC 777 notify, wrapped for tmux passthrough. It goes through the agent's own
# pane; "all" lets the pane pass it on while its window is hidden.
[ -w "$tty" ] || { log "notify: cannot write to $tty for $session"; exit 0; }
t set-option -p -t "$pane" allow-passthrough all
clean() { printf '%s' "$1" | tr -d '\000-\037;'; }
# shellcheck disable=SC1003  # \\ ends the escape sequence (ST)
printf '\033Ptmux;\033\033]777;notify;%s;%s\a\033\\' "$(clean "$session")" "$(clean "$body")" >"$tty"
