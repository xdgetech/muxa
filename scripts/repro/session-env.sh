#!/usr/bin/env bash
# Fact: new-session -e KEY=VALUE, sent on a control client's stdin, reaches the
# pane's process byte for byte, overrides the global environment for that
# session only, and reaches later windows of the session (ADR 0016).
set -euo pipefail
MUXA_SOCKET="muxa-repro-$$"
. "$(dirname "$0")/../lib/tmux.bash"
trap 'mt_down; rm -rf "$tmp"' EXIT
tmp="$(mktemp -d)"

mt new-session -d -s base -x 80 -y 20 sh
mt set-environment -g MUXA_REPRO_DIR /global
base="$(mt display-message -p -t '=base:' '#{session_id}')"

# The value as the bridge quotes it: \" \$ \\ escaped, ~ after the key.
dump="env > $tmp/first; sleep 5"
{
  # shellcheck disable=SC2016 # the $ is for tmux, escaped
  printf 'new-session -d -s envy -e "MUXA_REPRO_DIR=/per/session" -e "MUXA_REPRO_TOKEN=a b\\"c\\$d\\\\e~f" "%s"\n' "$dump"
  sleep 0.5
} | mt_control "$base" >/dev/null
wait_for "test -s $tmp/first" || fail "the session's command did not run"

grep -qx 'MUXA_REPRO_DIR=/per/session' "$tmp/first" || fail "-e did not override the global value"
pass "-e overrides the global environment for the new session's pane"
# shellcheck disable=SC2016 # a literal $
grep -qxF 'MUXA_REPRO_TOKEN=a b"c$d\e~f' "$tmp/first" || fail "the value changed on the way: $(grep MUXA_REPRO_TOKEN "$tmp/first")"
pass "a quoted value with space, quote, \$, backslash and ~ arrives byte for byte"

[ "$(mt show-environment -g MUXA_REPRO_DIR)" = "MUXA_REPRO_DIR=/global" ] || fail "the global environment changed"
mt show-environment -g MUXA_REPRO_TOKEN >/dev/null 2>&1 && fail "the value leaked into the global environment"
pass "the global environment is unchanged"

envy="$(mt display-message -p -t '=envy:' '#{session_id}')"
mt new-window -d -t "$envy:" "env > $tmp/second; sleep 5"
wait_for "test -s $tmp/second" || fail "the second window did not run"
grep -qx 'MUXA_REPRO_DIR=/per/session' "$tmp/second" || fail "a later window lost the session's value"
pass "a later window of the session gets the same values"
