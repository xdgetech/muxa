#!/usr/bin/env bash
# Fact: with the pane option remain-on-exit on and no pane-died hook, a pane
# whose process exits stays in place, dead. Its session stays, and a desktop
# client on it stays attached. respawn-pane -k with a command, sent on a
# control client's stdin, then restarts it with the same pane id, size and
# layout, and remain-on-exit stays on. The secretary's session uses this
# (ADR 0056): the daemon respawns its agent instead of opening a new session.
set -euo pipefail
: "${MUXA_SOCKET:=muxa-repro-$$}"
. "$(dirname "$0")/../lib/tmux.bash"
trap 'mt_down; rm -rf "$tmp"' EXIT
tmp="$(mktemp -d)"

mt new-session -d -s base -x 80 -y 20 sh
base="$(mt display-message -p -t '=base:' '#{session_id}')"
mt new-session -d -s chat -x 80 -y 20 "exec sleep 600"
chat="$(mt display-message -p -t '=chat:' '#{session_id}')"
pane="$(mt display-message -p -t '=chat:' '#{pane_id}')"
# As muxa_tmux::remain_on_exit sends it.
{ printf 'set-option -p -t %s remain-on-exit on\n' "$pane"; sleep 0.5; } | mt_control "$base" >/dev/null

mt_desktop "$chat" 100 30 8 &
wait_for "[ \"\$(mt list-clients -t '$chat' | wc -l)\" -ge 1 ]" || fail "no desktop client"
before="$(mt display-message -p -t "$pane" '#{pane_id} #{pane_width}x#{pane_height} #{window_layout}')"

kill "$(mt display-message -p -t "$pane" '#{pane_pid}')"
wait_for "[ \"\$(mt display-message -p -t '$pane' '#{pane_dead}')\" = 1 ]" || fail "the pane did not die"
mt has-session -t "$chat" 2>/dev/null || fail "the session closed with its pane"
[ "$(mt list-clients -t "$chat" | wc -l)" -ge 1 ] || fail "the desktop client left the session"
pass "with remain-on-exit and no hook, an exited pane stays dead; its session and client stay"

{
  printf 'respawn-pane -k -t %s -c "%s" -- "%s"\n' "$pane" "$tmp" "echo again > $tmp/ran; exec sleep 600"
  sleep 0.5
} | mt_control "$base" >/dev/null
wait_for "[ -f '$tmp/ran' ]" || fail "the respawned command did not run"
[ "$(mt display-message -p -t "$pane" '#{pane_dead}')" = 0 ] || fail "the pane is still dead"
after="$(mt display-message -p -t "$pane" '#{pane_id} #{pane_width}x#{pane_height} #{window_layout}')"
[ "$after" = "$before" ] || fail "the pane or layout changed: $before -> $after"
[ "$(mt show-options -pv -t "$pane" remain-on-exit)" = on ] || fail "remain-on-exit was reset"
pass "respawn-pane -k restarts a dead pane in place ($after); remain-on-exit stays on"
