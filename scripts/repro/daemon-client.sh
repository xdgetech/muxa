#!/usr/bin/env bash
# Fact: the daemon's control client, attached with -f ignore-size,no-output,
# is accepted, gets no %output, and changes no layout next to a desktop
# client, while it works and after it detaches (ADR 0019).
# MUXA_REPRO_SECONDS sets how long it stays attached (default 60).
set -euo pipefail
MUXA_SOCKET="muxa-repro-$$"
. "$(dirname "$0")/../lib/tmux.bash"
seconds="${MUXA_REPRO_SECONDS:-60}"
out="$(mktemp)"
trap 'rm -f "$out"; mt_down' EXIT

session="$(mt new-session -d -P -F '#{session_id}' -x 200 -y 50 sh)"
pane="$(mt display-message -p -t "$session" '#{pane_id}')"
mt split-window -d -t "$pane" -h sh
mt set-option -g window-size latest
mt set-option -g aggressive-resize on
layout() { mt list-panes -a -F '#{window_id} #{window_layout} #{pane_id} #{pane_width}x#{pane_height}'; }

# 1 and 2. Accepted, and no %output while the pane prints.
{
  sleep 0.5
  mt send-keys -t "$pane" 'echo daemon-client-marker' Enter
  sleep 1
  echo 'display-message -p ok'
  sleep 0.5
} | mt -C attach-session -t "$session" -f ignore-size,no-output >"$out" 2>&1 || true
grep -q '^%error' "$out" && fail "attach-session -f ignore-size,no-output was refused: $(head -3 "$out")"
grep -q '^ok$' "$out" || fail "the no-output client did not run a command: $(head -3 "$out")"
pass "attach-session -f ignore-size,no-output is accepted"
grep -q '^%output' "$out" && fail "a no-output client got %output"
{ sleep 0.5; mt send-keys -t "$pane" 'echo marker-two' Enter; sleep 1; } \
  | mt -C attach-session -t "$session" -f ignore-size >"$out" 2>&1 || true
grep -q '^%output' "$out" || skip "the control client without no-output got no %output either"
pass "with no-output, no %output arrives; without it, %output does"

# 3. Next to a desktop client, attached for $seconds s and running a command
# every 2 s, as the daemon's tick does.
mt_desktop "$session" 160 40 $((seconds + 10)) &
desktop=$!
wait_for "[ \"\$(window_size '$session')\" != 200x50 ]"
before="$(layout)"
: >"$out"
{
  for _ in $(seq $((seconds / 2))); do
    echo 'list-sessions -F "#{session_id}"'
    sleep 2
    [ "$(layout)" = "$before" ] || layout >>"$out"
  done
} | mt -C attach-session -t "$session" -f ignore-size,no-output >/dev/null 2>&1 || true
[ -s "$out" ] && fail "the layout changed while the daemon's client was attached: $(head -3 "$out")"
pass "a ${seconds} s ignore-size,no-output client next to a desktop client changes no layout"

# 4. It has detached now (its input ended); nothing moved after that either.
sleep 2
[ "$(layout)" = "$before" ] || fail "the layout changed after the daemon's client detached: $(layout)"
pass "detaching changes no layout"
kill "$desktop" 2>/dev/null || true
wait 2>/dev/null || true
