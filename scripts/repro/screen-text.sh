#!/usr/bin/env bash
# Fact: capture-pane -p without -e prints the screen as plain text, without
# color escapes, and -J joins a line the terminal wrapped.
set -euo pipefail
MUXA_SOCKET="muxa-repro-$$"
. "$(dirname "$0")/../lib/tmux.bash"
trap mt_down EXIT

pane="$(mt new-session -d -P -F '#{pane_id}' -x 40 -y 10 sh)"
mt send-keys -t "$pane" "clear; printf '\\033[1;32mGreen\\033[0m mark\\n%s\\n' \"\$(printf 'x%.0s' 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 18 19 20 21 22 23 24 25 26 27 28 29 30 31 32 33 34 35)wrapped-end\"" Enter
wait_for "mt capture-pane -p -t '$pane' | grep -q 'Green mark'"

mt capture-pane -p -t "$pane" | grep -q "$(printf '\033')" && fail "capture-pane -p carries escapes"
pass "capture-pane -p prints plain text"
mt capture-pane -p -t "$pane" | grep -q 'xwrapped-end' && fail "the wrapped line is joined without -J"
mt capture-pane -p -J -t "$pane" | grep -q 'xxxxxwrapped-end' || fail "capture-pane -J does not join the wrapped line"
pass "capture-pane -J joins a line the terminal wrapped"
