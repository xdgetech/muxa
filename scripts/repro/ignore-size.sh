#!/usr/bin/env bash
# Fact: a control client attached with -f ignore-size never resizes a window,
# next to a desktop client and also as the only client.
# Hazard: after `refresh-client -C <size>`, an ignore-size client that is the
# only client DOES size the window. muxa must never send refresh-client -C.
set -euo pipefail
MUXA_SOCKET="muxa-repro-$$"
. "$(dirname "$0")/../lib/tmux.bash"
trap mt_down EXIT

session="$(mt new-session -d -P -F '#{session_id}' -x 200 -y 50 sh)"
mt set-option -g window-size latest
mt set-option -g aggressive-resize on

# 1. Next to a desktop client.
mt_desktop "$session" 160 40 3 &
wait_for "[ \"\$(window_size '$session')\" != 200x50 ]"
desktop="$(window_size "$session")"
{ sleep 1; } | mt_control "$session" >/dev/null
[ "$(window_size "$session")" = "$desktop" ] || fail "window resized next to a desktop client: $desktop -> $(window_size "$session")"
pass "next to a desktop client, the window stays $desktop"

# 2. Alone, after the desktop client left: the usual case when you leave the desk.
wait
before="$(window_size "$session")"
{ sleep 1; } | mt_control "$session" >/dev/null
[ "$(window_size "$session")" = "$before" ] || fail "window resized when alone: $before -> $(window_size "$session")"
pass "as the only client, the window stays $before"

# 3. The hazard: the same, after refresh-client -C.
{ echo 'refresh-client -C 400x100'; sleep 1; } | mt_control "$session" >/dev/null
after="$(window_size "$session")"
[ "$after" != "$before" ] || skip "refresh-client -C did not resize on this tmux version"
pass "hazard confirmed: with refresh-client -C, a lone control client resized the window to $after"
