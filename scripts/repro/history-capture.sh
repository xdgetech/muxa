#!/usr/bin/env bash
# Fact: on a control client, capture-pane -p -e -S -N sends the last N
# scrollback lines and then the screen, with raw escape bytes for colors, and
# leaves the pane out of copy mode. history_size counts the lines kept.
set -euo pipefail
MUXA_SOCKET="muxa-repro-$$"
. "$(dirname "$0")/../lib/tmux.bash"
trap mt_down EXIT

mt new-session -d -s base -x 60 -y 10 sh
base="$(mt display-message -p -t '=base:' '#{session_id}')"
pane="$(mt display-message -p -t '=base:' '#{pane_id}')"
# shellcheck disable=SC2016 # expanded by the pane's shell
mt send-keys -t "$pane" -l 'i=1; while [ $i -le 50 ]; do printf "\033[31mrow %d\033[0m\n" $i; i=$((i+1)); done'
mt send-keys -t "$pane" Enter
wait_for "[ \"\$(mt display-message -p -t '$pane' '#{history_size}')\" -ge 40 ]" || fail "no scrollback"

out="$(printf 'capture-pane -p -e -t %s -S -20\n' "$pane" | mt_control "$base")"
printf '%s' "$out" | grep -q $'\033\\[31mrow 50' || fail "the screen or its colors are missing"
pass "the capture ends with the screen, with raw escape bytes"
printf '%s' "$out" | grep -q 'row 35' || fail "the scrollback is missing"
printf '%s' "$out" | grep -q 'row 20'$'\033' && fail "more than 20 scrollback lines came"
pass "-S -20 sends the last 20 scrollback lines"
[ "$(mt display-message -p -t "$pane" '#{pane_in_mode}')" = 0 ] || fail "the pane is in a mode"
pass "the pane stays out of copy mode"
