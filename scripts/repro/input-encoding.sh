#!/usr/bin/env bash
# Fact: a double-quoted argument with octal escapes carries any text intact
# through the control-mode command line, and send-keys -H passes raw bytes.
set -euo pipefail
MUXA_SOCKET="muxa-repro-$$"
. "$(dirname "$0")/../lib/tmux.bash"
trap mt_down EXIT

session="$(mt new-session -d -P -F '#{session_id}' -x 80 -y 20 sh)"

# a"b$c\d<TAB>e<LF>f é😀 ;{}#~ , written the way the bridge quotes it.
# shellcheck disable=SC2016  # the tmux-escaped text, not shell variables
quoted='"a\"b\$c\\d\011e\012f é😀 ;{}#~"'
expected=$'a"b$c\\d\te\nf é😀 ;{}#~'
{ echo "set-buffer -b muxa-repro -- $quoted"; sleep 0.3; } | mt_control "$session" >/dev/null
[ "$(mt show-buffer -b muxa-repro)" = "$expected" ] || fail "quoted text changed: $(mt show-buffer -b muxa-repro | od -c | head -3)"
pass "quoted text with quotes, \$, backslash, control bytes and UTF-8 round-trips"

pane="$(mt display-message -p -t "$session" '#{pane_id}')"
mt send-keys -t "$pane" 'clear; cat > /dev/null' Enter
sleep 0.3
mt send-keys -t "$pane" -H c3 a9 41
sleep 0.3
mt capture-pane -p -t "$pane" | grep -q 'éA' || fail "send-keys -H did not pass UTF-8 bytes"
pass "send-keys -H passes raw UTF-8 bytes"
