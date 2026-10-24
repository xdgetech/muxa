#!/usr/bin/env bash
# Fact: %output escapes only bytes below 0x20 and the backslash (as \ooo);
# other bytes, including invalid UTF-8, arrive raw. The bridge reads bytes.
set -euo pipefail
MUXA_SOCKET="muxa-repro-$$"
. "$(dirname "$0")/../lib/tmux.bash"
trap 'mt kill-server 2>/dev/null || true' EXIT

session="$(mt new-session -d -P -F '#{session_id}' -x 80 -y 20 sh)"
pane="$(mt display-message -p -t "$session" '#{pane_id}')"
out="$({
  sleep 0.3
  mt send-keys -t "$pane" "printf 'X\\377\\376Y\\\\Z\\n'" Enter
  sleep 0.5
} | mt_control "$session")"

LC_ALL=C grep -aqF $'X\xff\xfeY' <<<"$out" || fail "invalid UTF-8 was not passed raw"
pass "bytes 0xff 0xfe arrive raw in %output"
LC_ALL=C grep -aqF 'Y\134Z' <<<"$out" || fail "the backslash was not escaped as \134"
pass "the backslash arrives as \134"
