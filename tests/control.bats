#!/usr/bin/env bats
# A control-mode client (tmux -C attach -f ignore-size), such as a bridge to
# another device, must not change sidebars, pane sizes, or agent states.
load helper

setup() { setup_server; load_plugin; }
teardown() { exec 4>&- 2>/dev/null || true; teardown_server; }

sidebars_in() { tx list-panes -t "$1" -F '#{@agents_sidebar_pane}' | grep -c '^1$' || true; }
sizes() { tx list-panes -a -F '#{pane_id} #{@agents_sidebar_pane} #{pane_width}x#{pane_height}'; }

# Attaches a control client to base. Commands written to fd 4 go to it.
attach_control() {
  mkfifo "$TMPDIR/ctl"
  tx -C attach -t base -f ignore-size <"$TMPDIR/ctl" >/dev/null 3>&- &
  exec 4>"$TMPDIR/ctl"
  wait_for '[ "$(tx list-clients -F "#{client_control_mode}")" = 1 ]'
  sleep 1  # the hooks run in the background
}

@test "a control client keeps every sidebar and pane size" {
  wait_for '[ "$(sidebars_in base:)" -eq 1 ]'
  before="$(sizes)"
  attach_control
  [ "$(sidebars_in base:)" -eq 1 ]
  [ "$(sizes)" = "$before" ]
}

@test "a control client does not mark a finished turn as seen" {
  work="$(tx list-panes -t base: -F '#{pane_id} #{@agents_sidebar_pane}' | awk '$2!=1{print $1}')"
  "$ROOT/scripts/state.sh" set "$work" done
  attach_control
  "$ROOT/scripts/state.sh" seen
  [ "$(pane_opt "$work" @agent_state)" = done ]
}

@test "a window made by a control client gets a sidebar" {
  attach_control
  echo "new-window -d -t base: -n made-by-control" >&4
  wait_for "tx list-windows -t base: -F '#{window_name}' | grep -qx made-by-control"
  wait_for '[ "$(sidebars_in base:made-by-control)" -eq 1 ]'
}
