#!/usr/bin/env bash
# Fact: a program in a pane that prints CSI 3J ("erase saved lines") makes tmux
# drop the pane's whole history. Agent UIs print it when they redraw after a
# resize, so a window sized for a device can lose its scrollback.
set -euo pipefail
MUXA_SOCKET="muxa-repro-$$"
. "$(dirname "$0")/../lib/tmux.bash"
trap mt_down EXIT

mt new-session -d -s base -x 60 -y 10 sh
pane="$(mt display-message -p -t '=base:' '#{pane_id}')"
mt send-keys -t "$pane" -l 'seq 1 200'
mt send-keys -t "$pane" Enter
wait_for "[ \"\$(mt display-message -p -t '$pane' '#{history_size}')\" -ge 150 ]" || fail "no scrollback"
pass "the pane keeps its scrollback"

mt send-keys -t "$pane" -l "printf '\\033[H\\033[2J\\033[3J'"
mt send-keys -t "$pane" Enter
wait_for "[ \"\$(mt display-message -p -t '$pane' '#{history_size}')\" -lt 5 ]" || fail "the history stayed"
pass "CSI 3J drops the pane's history"
