#!/usr/bin/env bash
# Fact: what a sized control client (refresh-client -C, no ignore-size) can
# resize, and what it cannot (ADR 0020).
# - It sizes the windows of its own session only. When no other client views
#   that session, it sizes every window of it, also windows it does not show.
# - Those windows keep its size after it leaves, until a desktop client views them.
# - window-size smallest lets it shrink a window the desktop is typing in;
#   largest and manual keep the desktop's size. Only latest follows the device.
# - A per-window size (refresh-client -C @id:WxH, tmux 3.3+) caps the window
#   in every mode, so it cannot follow the latest client.
# - A control client with ignore-size and no size (the daemon's) sizes nothing,
#   also when it is the only client.
set -euo pipefail
MUXA_SOCKET="muxa-repro-$$"
. "$(dirname "$0")/../lib/tmux.bash"
log="$(mktemp)"
trap 'rm -f "$log"; mt_down' EXIT

home="$(mt new-session -d -P -F '#{session_id}' -x 200 -y 50 sh)"
task="$(mt new-session -d -P -F '#{session_id}' sh)"
mt new-window -d -t "$task" sh
other="$(mt list-windows -t "$task" -F '#{window_id}' | tail -1)"
mt_desktop "$home" 160 40 60 &
desktop=$!
wait_for "[ \"\$(window_size '$home')\" = 160x39 ]"

# 1. On the task session, while the desktop views another session.
mt_control_open 3 "$task"
echo 'refresh-client -C 120x35' >&3
wait_for "[ \"\$(window_size '$task')\" = 120x35 ]" || fail "the task window is $(window_size "$task"), expected 120x35"
[ "$(window_size "$other")" = 120x35 ] || fail "the task's other window is $(window_size "$other"), expected 120x35"
[ "$(window_size "$home")" = 160x39 ] || fail "the desktop's session changed to $(window_size "$home")"
pass "a sized client sizes every window of its own session (120x35), and no other session"

# 2. It leaves: the windows keep its size until the desktop views them.
exec 3>&-
sleep 0.7
[ "$(window_size "$task")" = 120x35 ] || fail "the task window changed to $(window_size "$task") with no client"
pass "after it leaves, its session's windows keep 120x35 while no client views them"
mt switch-client -c "$(mt list-clients -F '#{client_name}' | head -1)" -t "$task"
wait_for "[ \"\$(window_size '$task')\" = 160x39 ]" || fail "the desktop did not size the task window: $(window_size "$task")"
pass "a desktop client that switches to the session sizes its current window: 160x39"

# 3. Sharing the desktop's session, under each window-size value. The desktop
# is the latest client again before each check.
mt_control_open 3 "$task"
echo 'refresh-client -C 120x35' >&3
wait_for "[ \"\$(window_size '$task')\" = 120x35 ]" || fail "attaching did not make the control client latest: $(window_size "$task")"
pass "attaching to the desktop's session makes it latest: the desktop's window is 120x35"
check_mode() { # <window-size> <expected size>
  mt set-option -g window-size "$1"
  echo 'refresh-client -C 120x35' >&3
  sleep 0.5
  [ "$(window_size "$task")" = "$2" ] || fail "window-size $1: the window is $(window_size "$task"), expected $2"
}
check_mode smallest 120x35
check_mode largest 160x39
pass "window-size smallest gives the window the smaller control client's 120x35; largest keeps the desktop's 160x39"
mt set-option -g window-size manual
mt resize-window -t "$task" -x 150 -y 30
check_mode manual 150x30
pass "window-size manual keeps the window's own size (150x30)"
mt set-option -g window-size latest
# resize-window pins that one window to manual; #{window-size} shows it.
[ "$(mt display-message -p -t "$task" '#{window-size}')" = manual ] || fail "resize-window did not set the window's window-size to manual"
mt set-option -wu -t "$task" window-size
[ "$(mt display-message -p -t "$task" '#{window-size}')" = latest ] || fail "#{window-size} does not show the inherited latest"
pass "resize-window sets that window's window-size to manual; #{window-size} shows the value in effect"
echo "switch-client -t $task ; refresh-client -C 120x35" >&3
wait_for "[ \"\$(window_size '$task')\" = 120x35 ]"

# 4. A per-window size caps the window even when the desktop is latest.
window="$(mt display-message -p -t "$task" '#{window_id}')"
: >"$log"
exec 3>&-
sleep 0.5
MT_CONTROL_LOG="$log" mt_control_open 3 "$task"
echo "refresh-client -C $window:90x20" >&3
sleep 0.7
if grep -q '^%error' "$log"; then
  echo "SKIP  per-window sizes are not supported by tmux $(mt display-message -p '#{version}')"
else
  wait_for "[ \"\$(window_size '$task')\" = 90x20 ]" || fail "the per-window size did not apply: $(window_size "$task")"
  mt switch-client -c "$(mt list-clients -F '#{client_name}' | grep -v '^client-' | head -1)" -t "$task" 2>/dev/null || true
  sleep 0.5
  [ "$(window_size "$task")" = 90x20 ] || fail "the per-window size did not cap the window: $(window_size "$task")"
  pass "a per-window size (refresh-client -C @id:90x20) caps the window at 90x20, whoever is latest"
fi
exec 3>&-
sleep 0.5

# 5. The daemon's client: ignore-size, no-output, no size. Alone, it sizes nothing.
kill "$desktop" 2>/dev/null || true
wait "$desktop" 2>/dev/null || true
lone="$(mt new-session -d -P -F '#{session_id}' -x 70 -y 20 sh)"
mt_control_open 3 "$lone" ignore-size,no-output
echo 'list-sessions' >&3
sleep 0.7
[ "$(window_size "$lone")" = 70x20 ] || fail "the daemon's client resized a window to $(window_size "$lone")"
exec 3>&-
pass "an ignore-size,no-output client with no size, alone, leaves the window at 70x20"
