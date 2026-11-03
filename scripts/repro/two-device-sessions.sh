#!/usr/bin/env bash
# Fact: two device sessions may link the same window, as two app windows or two
# devices watching it do (ADR 0021, 0029). Under window-size latest, with
# aggressive-resize off and on:
# - the device client that switched last sizes the window; the other one's
#   switch-client (on typing) takes it back;
# - neither device session changes the other one or the user's session: the
#   desktop's session, current window, its window's size and layout stay;
# - when the latest device gives the window up (ignore-size, then kill-session),
#   the window takes the remaining device's size at once, not the desktop's.
set -euo pipefail
MUXA_SOCKET="muxa-repro-$$"
. "$(dirname "$0")/../lib/tmux.bash"
trap 'exec 3>&- 4>&- 5>&- 6>&- 2>/dev/null || true; mt_down' EXIT

# A device session holding <window>; sets $device.
device_session() {
  local out
  out="$(mt new-session -d -P -F '#{session_id} #{window_id}' cat)"
  device="${out% *}"
  mt set-option -t "$device" detach-on-destroy on
  mt link-window -d -s "$1" -t "$device:" \; kill-window -t "${out#* }"
}

# The device client on fd $1 in session $2 takes its window at $3; then the
# bridge's own client (fd 3) switches to the target again.
take() {
  echo "refresh-client -f '!ignore-size' ; switch-client -E -t $2 ; refresh-client -C $3" >&"$1"
  sleep 0.3
  echo "switch-client -E -t $target" >&3
  sleep 0.3
}

desktop_view() { mt list-clients -F '#{client_session} #{window_id} #{pane_id}' -f '#{!=:#{client_control_mode},1}'; }

check() { # <aggressive-resize>
  local agg="$1" home watched view layout first second
  target="$(mt new-session -d -P -F '#{session_id}' -x 160 -y 39 sh)"
  mt set-option -g aggressive-resize "$agg"
  home="$(mt display-message -p -t "$target" '#{window_id}')"
  watched="$(mt new-window -d -P -F '#{window_id}' -t "$target" sh)"
  mt split-window -d -h -t "$watched" sh
  exec 6> >(mt_desktop "$target" 160 40 60 stdin)
  wait_for "[ -n \"\$(mt list-clients -F x)\" ]"
  mt_control_open 3 "$target" ignore-size
  sleep 0.3
  echo x >&6
  sleep 0.3
  view="$(desktop_view)"
  layout="$(mt display-message -p -t "$home" '#{window_layout}')"

  device_session "$watched"
  first="$device"
  mt_control_open 4 "$first" ignore-size,no-output
  device_session "$watched"
  second="$device"
  mt_control_open 5 "$second" ignore-size,no-output
  sleep 0.2

  take 4 "$first" 100x30
  wait_for "[ \"\$(window_size '$watched')\" = 100x30 ]" || fail "aggressive-resize $agg: the first device did not size the window: $(window_size "$watched")"
  take 5 "$second" 120x35
  wait_for "[ \"\$(window_size '$watched')\" = 120x35 ]" || fail "aggressive-resize $agg: the second device did not take the window: $(window_size "$watched")"
  [ "$(mt list-windows -t "$first" -F '#{window_id}')" = "$watched" ] || fail "aggressive-resize $agg: the first device session changed"
  [ "$(mt list-windows -t "$second" -F '#{window_id}')" = "$watched" ] || fail "aggressive-resize $agg: the second device session changed"
  pass "aggressive-resize $agg: two device sessions hold one window; the one that switched last sizes it (120x35)"

  take 4 "$first" 100x30
  wait_for "[ \"\$(window_size '$watched')\" = 100x30 ]" || fail "aggressive-resize $agg: the first device's switch-client did not take it back: $(window_size "$watched")"
  [ "$(window_size "$home")" = 160x39 ] || fail "aggressive-resize $agg: the desktop's window changed to $(window_size "$home")"
  [ "$(desktop_view)" = "$view" ] || fail "aggressive-resize $agg: the desktop client moved"
  [ "$(mt display-message -p -t "$home" '#{window_layout}')" = "$layout" ] || fail "aggressive-resize $agg: the desktop's layout changed"
  pass "aggressive-resize $agg: the other device's switch-client takes it back (100x30); the desktop's window, view and layout stay"

  # The latest device leaves, as a bridge does on unwatch or exit.
  echo "refresh-client -f ignore-size ; refresh-client -C 100x30" >&4
  sleep 0.3
  mt kill-session -t "$first"
  exec 4>&-
  wait_for "[ \"\$(window_size '$watched')\" = 120x35 ]" || fail "aggressive-resize $agg: the window did not go to the remaining device: $(window_size "$watched")"
  [ "$(window_size "$home")" = 160x39 ] || fail "aggressive-resize $agg: the desktop's window changed to $(window_size "$home")"
  [ "$(desktop_view)" = "$view" ] || fail "aggressive-resize $agg: the desktop client moved"
  pass "aggressive-resize $agg: when the latest device leaves, the window takes the remaining device's size (120x35)"

  exec 5>&- 3>&- 6>&-
  sleep 0.3
  mt kill-server
}

check off
check on
