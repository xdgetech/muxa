#!/usr/bin/env bash
# Fact: a pane-option pane-died hook with remain-on-exit, set on a control
# client's stdin, gives the pane a login shell in place when its process
# exits. The hook removes itself, so the shell's own exit closes the pane and
# the session. respawn-pane -k does not fire the hook, and the hook survives it.
set -euo pipefail
MUXA_SOCKET="muxa-repro-$$"
. "$(dirname "$0")/../lib/tmux.bash"
trap 'mt_down; rm -rf "$tmp"' EXIT
tmp="$(mktemp -d)"
mkdir "$tmp/work"

mt new-session -d -s base -x 80 -y 20 sh
base="$(mt display-message -p -t '=base:' '#{session_id}')"
mt new-session -d -s task -c "$tmp/work" "exec sleep 600"
pane="$(mt display-message -p -t '=task:' '#{pane_id}')"
shell="$(mt show-options -gv default-shell)"

# As muxa_tmux::shell_after_exit sends it.
once="set-hook -pu -t $pane pane-died ; set-option -pu -t $pane remain-on-exit ; respawn-pane -k -t $pane -c \\\"$tmp/work\\\" -- \\\"$shell\\\" -l"
{
  printf 'set-hook -p -t %s pane-died "%s"\n' "$pane" "$once"
  printf 'set-option -p -t %s remain-on-exit on\n' "$pane"
  sleep 0.5
} | mt_control "$base" >/dev/null
[ -n "$(mt show-hooks -p -t "$pane" pane-died)" ] || fail "the hook was not set"

# A restart replaces the process without firing the hook.
mt respawn-pane -k -t "$pane" "exec sleep 1"
sleep 0.3
[ "$(mt display-message -p -t "$pane" '#{pane_current_command}')" = sleep ] || fail "respawn-pane fired the hook"
[ -n "$(mt show-hooks -p -t "$pane" pane-died)" ] || fail "respawn-pane removed the hook"
pass "respawn-pane -k does not fire the pane-died hook, and the hook stays"

wait_for "[ \"\$(mt display-message -p -t '$pane' '#{pane_current_command}')\" != sleep ]" || fail "the process did not exit"
[ "$(mt display-message -p -t "$pane" '#{pane_id} #{pane_dead} #{session_name}')" = "$pane 0 task" ] \
  || fail "the pane did not get a shell: $(mt display-message -p -t "$pane" '#{pane_id} #{pane_dead}')"
[ "$(mt display-message -p -t "$pane" '#{pane_start_command}')" = "$shell -l" ] || fail "the pane does not run $shell -l"
here="$(cd "$tmp/work" && pwd -P)"
[ "$(cd "$(mt display-message -p -t "$pane" '#{pane_current_path}')" && pwd -P)" = "$here" ] || fail "the shell is not in the task directory"
pass "the exit leaves the same pane alive with a login shell in its directory"

[ -z "$(mt show-hooks -p -t "$pane" pane-died)" ] || fail "the hook did not remove itself"
mt send-keys -t "$pane" exit Enter
wait_for "! mt has-session -t '=task' 2>/dev/null" || fail "the shell's exit did not close the session"
pass "the hook fires once: the shell's exit closes the session"
