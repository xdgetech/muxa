#!/usr/bin/env bash
# Fact: in a "a ; b ; c" command line, each command gets its own %begin/%end
# block. After a runtime %error the rest of the line is skipped with no block.
# A parse error gives one %error block for the whole line.
set -euo pipefail
MUXA_SOCKET="muxa-repro-$$"
. "$(dirname "$0")/../lib/tmux.bash"
trap mt_down EXIT

session="$(mt new-session -d -P -F '#{session_id}' -x 80 -y 20 sh)"
blocks() { grep -cE '^%(end|error) [0-9]+ [0-9]+ 1$' <<<"$1" || true; }

out="$(printf '%s\n' 'display-message -p one ; display-message -p two ; display-message -p three' | mt_control "$session")"
[ "$(blocks "$out")" = 3 ] || fail "expected 3 blocks for 3 commands, got $(blocks "$out")"
pass "one block per command in a list"

out="$(printf '%s\n' 'display-message -p one ; capture-pane -p -t %999 ; display-message -p three' | mt_control "$session")"
[ "$(blocks "$out")" = 2 ] || fail "expected 2 blocks (ok, error), got $(blocks "$out")"
grep -q '^three$' <<<"$out" && fail "the command after a failed one ran"
pass "after a runtime error, the rest of the list is skipped and sends no block"

out="$(printf '%s\n' 'display-message -p one ; no-such-command ; display-message -p three' | mt_control "$session")"
[ "$(blocks "$out")" = 1 ] || fail "expected 1 block for a parse error, got $(blocks "$out")"
pass "a parse error gives one block for the whole line"
