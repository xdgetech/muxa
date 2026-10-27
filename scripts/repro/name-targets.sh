#!/usr/bin/env bash
# Fact: a name target is ambiguous. "-t b" can resolve to a window whose name
# starts with "b" in the current session instead of session "b". Ids are exact.
set -euo pipefail
MUXA_SOCKET="muxa-repro-$$"
. "$(dirname "$0")/../lib/tmux.bash"
trap mt_down EXIT

mt new-session -d -s b -x 80 -y 20 sh
mt new-session -d -s a -n bash -x 80 -y 20 sh
a="$(mt display-message -p -t '=a:' '#{session_id}')"

out="$({ echo 'display-message -p -t b "#{session_name}"'; sleep 0.3; } | mt_control "$a")"
if grep -qx a <<<"$out"; then
  pass "-t b resolved to window 'bash' in session a, not session b"
else
  skip "-t b resolved to session b on this tmux version; ids are still required"
fi
out="$({ echo "display-message -p -t $a \"#{session_name}\""; sleep 0.3; } | mt_control "$a")"
grep -qx a <<<"$out" || fail "an id target did not resolve exactly"
pass "an id target ($a) is exact"
