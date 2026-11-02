#!/usr/bin/env bash
# Fact: respawn-pane -k, sent on a control client's stdin, restarts one pane's
# process in place. It keeps the pane id, the pane's size and the window
# layout, selects nothing, and moves no client. Without a command it reruns
# the pane's start command; -e and -c reach the new process, and the
# session's environment stays (ADR 0018).
set -euo pipefail
MUXA_SOCKET="muxa-repro-$$"
. "$(dirname "$0")/../lib/tmux.bash"
trap 'mt_down; rm -rf "$tmp"' EXIT
tmp="$(mktemp -d)"
mkdir "$tmp/work"

# An agent pane started with a command, as the built-in launcher does, beside
# a shell pane, in a window that is not the current one, in a session a
# desktop client watches.
mt new-session -d -s base -x 80 -y 20 sh
base="$(mt display-message -p -t '=base:' '#{session_id}')"
{
  printf 'new-session -d -s task -e "MUXA_REPRO_SESSION=kept" "%s"\n' \
    "echo run >> $tmp/runs; env > $tmp/env; pwd > $tmp/pwd; exec sleep 600"
  sleep 0.5
} | mt_control "$base" >/dev/null
task="$(mt display-message -p -t '=task:' '#{session_id}')"
agent="$(mt display-message -p -t "$task:0.0" '#{pane_id}')"
mt new-window -d -t "$task:" sh
shell="$(mt split-window -d -h -t "$agent" -P -F '#{pane_id}')"
wait_for "[ \"\$(wc -l < $tmp/runs)\" -eq 1 ]" || fail "the first run did not start"

mt_desktop "$task" 120 30 6 &
wait_for "[ \"\$(mt list-clients -t '$task' | wc -l)\" -ge 1 ]" || fail "no desktop client"
sleep 0.5
mt select-window -t "$task:1"
mt select-pane -t "$shell"

state() {
  mt display-message -p -t "$agent" \
    '#{pane_id} #{pane_width}x#{pane_height} #{window_layout} #{pane_active} #{window_active}'
}
selection() {
  mt display-message -p -t "$task:" '#{window_id} #{pane_id}'
  mt list-clients -F '#{client_tty} #{session_id} #{window_id}'
}
before="$(state)"
selected="$(selection)"
old_pid="$(mt display-message -p -t "$agent" '#{pane_pid}')"

{
  # shellcheck disable=SC2016 # the $ is for tmux, escaped
  printf 'respawn-pane -k -t %s -c "%s" -e "MUXA_REPRO_WORKSPACE=a b\\"c\\$d"\n' "$agent" "$tmp/work"
  sleep 0.5
} | mt_control "$base" >/dev/null
wait_for "[ \"\$(wc -l < $tmp/runs)\" -eq 2 ]" || fail "the start command did not run again"
pass "without a command, respawn-pane -k reruns the pane's start command"

[ "$(mt display-message -p -t "$agent" '#{pane_pid}')" != "$old_pid" ] || fail "the process was not replaced"
after="$(state)"
[ "$after" = "$before" ] || fail "the pane or layout changed: $before -> $after"
pass "the pane id, its size and the window layout are unchanged ($before)"

[ "$(selection)" = "$selected" ] || fail "the selection moved: $selected -> $(selection)"
pass "no window or pane is selected, and no client moves"

# shellcheck disable=SC2016 # a literal $
grep -qxF 'MUXA_REPRO_WORKSPACE=a b"c$d' "$tmp/env" || fail "-e did not reach the new process"
grep -qx 'MUXA_REPRO_SESSION=kept' "$tmp/env" || fail "the session's environment was lost"
pass "-e reaches the new process byte for byte, and the session's environment stays"

[ "$(cat "$tmp/pwd")" = "$(cd "$tmp/work" && pwd -P)" ] || [ "$(cat "$tmp/pwd")" = "$tmp/work" ] \
  || fail "-c was not the working directory: $(cat "$tmp/pwd")"
pass "-c sets the new process's working directory"

# A pane started as a plain shell, as tmux-agents does before it types the
# agent command, has no start command: respawn-pane then starts a shell.
[ -z "$(mt display-message -p -t "$shell" '#{pane_start_command}')" ] || skip "this tmux records a start command for a plain pane"
mt send-keys -t "$shell" -l "exec sleep 600" && mt send-keys -t "$shell" Enter
wait_for "[ \"\$(mt display-message -p -t '$shell' '#{pane_current_command}')\" = sleep ]" || fail "the typed command did not run"
{ printf 'respawn-pane -k -t %s\n' "$shell"; sleep 0.5; } | mt_control "$base" >/dev/null
wait_for "[ \"\$(mt display-message -p -t '$shell' '#{pane_current_command}')\" != sleep ]" || fail "the pane was not respawned"
pass "a pane without a start command gets a fresh shell: a typed agent must be typed again"
