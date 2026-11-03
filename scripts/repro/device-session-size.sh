#!/usr/bin/env bash
# Fact: a sized control client in a device session (one window, linked from
# the user's session) sizes that window only, also when it is not the
# desktop's current window (ADR 0021). Checked with aggressive-resize off and
# on, under window-size latest:
# - the desktop's session, current window, its window's size and layout stay;
# - a desktop key press in another window leaves it; in the same window it
#   takes it back; switch-client by the device client takes it again;
# - refresh-client -f ignore-size gives it back at once with aggressive-resize
#   off; with it on, when a desktop client next views the window;
# - a second device session for another window leaves the first window alone.
#   A grouped session may not: its client is a candidate for every window,
#   and may size those whose latest client is gone (aggressive-resize off;
#   in most runs, not all);
# - the daemon's ignore-size,no-output client sizes nothing meanwhile.
set -euo pipefail
MUXA_SOCKET="muxa-repro-$$"
. "$(dirname "$0")/../lib/tmux.bash"
trap 'exec 3>&- 4>&- 5>&- 6>&- 7>&- 2>/dev/null || true; mt_down' EXIT

# A device session holding <window>; sets $device.
device_session() {
  local out
  out="$(mt new-session -d -P -F '#{session_id} #{window_id}' cat)"
  device="${out% *}"
  mt link-window -d -s "$1" -t "$device:" \; kill-window -t "${out#* }"
}

# The device client on fd $1 in session $2 takes its window at 100x30; then the
# bridge's own client (fd 3) switches to the target again.
take() {
  echo "refresh-client -f '!ignore-size' ; switch-client -E -t $2 ; refresh-client -C 100x30" >&"$1"
  mt_control_sync "$1"
  echo "switch-client -E -t $target" >&3
  mt_control_sync 3
}

check() { # <aggressive-resize>
  local agg="$1" home watched view layout first second
  target="$(mt new-session -d -P -F '#{session_id}' -x 160 -y 39 sh)"
  mt set-option -g aggressive-resize "$agg"
  home="$(mt display-message -p -t "$target" '#{window_id}')"
  watched="$(mt new-window -d -P -F '#{window_id}' -t "$target" sh)"
  mt split-window -d -h -t "$watched" sh
  exec 5> >(mt_desktop "$target" 160 40 600 stdin)
  wait_for "[ -n \"\$(mt list-clients -F x)\" ]"
  # The daemon's client, and the bridge's own client, on the target.
  mt_control_open 6 "$target" ignore-size,no-output
  mt_control_open 3 "$target" ignore-size
  mt_control_sync 6
  mt_control_sync 3
  mt_desktop_type 5
  view="$(mt list-clients -F '#{client_session} #{window_id} #{pane_id} #{client_flags}' | grep -v control-mode)"
  layout="$(mt display-message -p -t "$home" '#{window_layout}')"

  device_session "$watched"
  first="$device"
  mt_control_open 4 "$first" ignore-size,no-output
  mt_control_sync 4
  take 4 "$first"
  wait_for "[ \"\$(window_size '$watched')\" = 100x30 ]" || fail "aggressive-resize $agg: the watched window is $(window_size "$watched")"
  [ "$(window_size "$home")" = 160x39 ] || fail "aggressive-resize $agg: the desktop's window changed to $(window_size "$home")"
  [ "$(mt list-clients -F '#{client_session} #{window_id} #{pane_id} #{client_flags}' | grep -v control-mode)" = "$view" ] || fail "aggressive-resize $agg: the desktop client moved"
  [ "$(mt display-message -p -t "$home" '#{window_layout}')" = "$layout" ] || fail "aggressive-resize $agg: the desktop's layout changed"
  [ "$(mt display-message -p '#{session_id}')" = "$target" ] || fail "aggressive-resize $agg: the device session is the best session"
  pass "aggressive-resize $agg: the device client sizes the watched window (100x30); the desktop's window ($home), view and layout stay"

  mt_desktop_type 5
  [ "$(window_size "$watched")" = 100x30 ] || fail "aggressive-resize $agg: a key press in another window took it: $(window_size "$watched")"
  mt select-window -t "$target:$watched"
  mt_desktop_type 5
  wait_for "[ \"\$(window_size '$watched')\" = 160x39 ]" || fail "aggressive-resize $agg: a desktop key press in the window left it at $(window_size "$watched")"
  take 4 "$first"
  wait_for "[ \"\$(window_size '$watched')\" = 100x30 ]" || fail "aggressive-resize $agg: switch-client did not take the window again"
  pass "aggressive-resize $agg: a desktop key press in another window leaves it; in the same window takes it back; switch-client takes it again"

  mt select-window -t "$target:$home"
  mt_desktop_type 5
  echo "refresh-client -f ignore-size ; refresh-client -C 100x30" >&4
  mt_control_sync 4
  if [ "$agg" = off ]; then
    [ "$(window_size "$watched")" = 160x39 ] || fail "aggressive-resize off: ignore-size did not give the window back: $(window_size "$watched")"
    pass "aggressive-resize off: refresh-client -f ignore-size gives it back at once (160x39)"
  else
    [ "$(window_size "$watched")" = 100x30 ] || fail "aggressive-resize on: the window changed with no client viewing it: $(window_size "$watched")"
  fi
  mt kill-session -t "$first"
  mt_control_close 4
  if [ "$agg" = on ]; then
    mt select-window -t "$target:$watched"
    wait_for "[ \"\$(window_size '$watched')\" = 160x39 ]" || fail "aggressive-resize on: viewing the window did not size it"
    pass "aggressive-resize on: the window keeps the device's size until a desktop client views it (160x39)"
    mt select-window -t "$target:$home"
  fi

  # A second device session, for the desktop's window: the first window,
  # whose latest client is gone, stays.
  device_session "$home"
  second="$device"
  mt_control_open 4 "$second" ignore-size,no-output
  mt_control_sync 4
  take 4 "$second"
  wait_for "[ \"\$(window_size '$home')\" = 100x30 ]" || fail "aggressive-resize $agg: the second device did not size its window"
  [ "$(window_size "$watched")" = 160x39 ] || fail "aggressive-resize $agg: the second device sized the first window: $(window_size "$watched")"
  pass "aggressive-resize $agg: a device session for another window leaves the first one at 160x39"
  mt kill-session -t "$second"
  mt_control_close 4

  # The same with a grouped session: it may size the first window too.
  if [ "$agg" = off ]; then
    mt select-window -t "$target:$home"
    mt_desktop_type 5
    device_session "$watched"
    first="$device"
    mt_control_open 4 "$first" ignore-size,no-output
    mt_control_sync 4
    take 4 "$first"
    echo "refresh-client -f ignore-size ; refresh-client -C 100x30" >&4
    mt_control_sync 4
    mt kill-session -t "$first"
    mt_control_close 4
    [ "$(window_size "$watched")" = 160x39 ] || fail "the first window did not return to 160x39"
    grouped="$(mt new-session -d -P -F '#{session_id}' -t "$target")"
    mt select-window -t "$grouped:$home"
    mt_control_open 7 "$grouped" ignore-size,no-output
    mt_control_sync 7
    take 7 "$grouped"
    if wait_for "[ \"\$(window_size '$watched')\" = 100x30 ]"; then
      pass "aggressive-resize off: a grouped session's client also sized a window whose latest client is gone (100x30): a device session must hold one window only"
    elif [ "$(window_size "$watched")" = 160x39 ]; then
      pass "aggressive-resize off: a grouped session's client left the first window at 160x39 this run (it may size it, and does in most runs)"
    else
      fail "the grouped session's client left the first window at $(window_size "$watched")"
    fi
    exec 7>&-
  fi
  exec 3>&- 6>&- 5>&-
  sleep 0.3
  mt kill-server
}

check off
check on
