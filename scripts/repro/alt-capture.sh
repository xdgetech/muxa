#!/usr/bin/env bash
# Fact: while a program uses the alternate screen, capture-pane shows the
# alternate screen and capture-pane -a shows the primary one.
set -euo pipefail
MUXA_SOCKET="muxa-repro-$$"
. "$(dirname "$0")/../lib/tmux.bash"
trap 'mt kill-server 2>/dev/null || true' EXIT

pane="$(mt new-session -d -P -F '#{pane_id}' -x 60 -y 10 sh)"
mt send-keys -t "$pane" 'echo PRIMARY-TEXT; printf "\033[?1049h\033[2;3HALT-TEXT"; sleep 30' Enter
wait_for "[ \"\$(mt display-message -p -t '$pane' '#{alternate_on}')\" = 1 ]"

mt capture-pane -p -t "$pane" | grep -q ALT-TEXT || fail "capture-pane does not show the alternate screen"
pass "capture-pane shows the alternate screen"
mt capture-pane -p -a -t "$pane" | grep -q PRIMARY-TEXT || fail "capture-pane -a does not show the primary screen"
pass "capture-pane -a shows the primary screen"
