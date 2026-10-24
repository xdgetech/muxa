#!/usr/bin/env bash
# Fact: a control client gets %output only for panes in its current session;
# after switch-client it gets the new session's panes.
set -euo pipefail
MUXA_SOCKET="muxa-repro-$$"
. "$(dirname "$0")/../lib/tmux.bash"
trap 'mt kill-server 2>/dev/null || true' EXIT

a="$(mt new-session -d -P -F '#{session_id}' -x 80 -y 20 sh)"
b_pane="$(mt new-session -d -P -F '#{pane_id}' -x 80 -y 20 sh)"
b="$(mt display-message -p -t "$b_pane" '#{session_id}')"

out="$({
  sleep 0.3; mt send-keys -t "$b_pane" 'echo before-switch' Enter; sleep 0.5
  echo "switch-client -t $b"; sleep 0.3
  mt send-keys -t "$b_pane" 'echo after-switch' Enter; sleep 0.5
} | mt_control "$a")"

grep -q "^%output $b_pane .*before-switch" <<<"$out" && fail "got output of a pane outside the current session"
pass "no %output for a pane in another session"
grep -q "^%output $b_pane .*after-switch" <<<"$out" || fail "no output after switch-client"
pass "after switch-client, %output arrives for that session's panes"
