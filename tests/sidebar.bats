#!/usr/bin/env bats
load helper

setup() { setup_server; load_plugin; }
teardown() { teardown_server; }

sidebars_in() { tx list-panes -t "$1" -F '#{@agents_sidebar_pane}' | grep -c '^1$' || true; }

@test "loading the plugin adds a sidebar to existing windows" {
  wait_for '[ "$(sidebars_in base:)" -eq 1 ]'
}

@test "a new window gets exactly one sidebar" {
  w="$(tx new-window -d -t base: -P -F '#{window_id}')"
  wait_for '[ "$(sidebars_in "$w")" -eq 1 ]'
  sleep 0.5
  [ "$(sidebars_in "$w")" -eq 1 ]
}

@test "the sidebar shows sessions and updates on a state change" {
  wait_for '[ "$(sidebars_in base:)" -eq 1 ]'
  sb="$(tx list-panes -t base: -F '#{pane_id} #{@agents_sidebar_pane}' | awk '$2==1{print $1}')"
  wait_for "tx capture-pane -p -t '$sb' | grep -q 'base'"
  work="$(tx list-panes -t base: -F '#{pane_id} #{@agents_sidebar_pane}' | awk '$2!=1{print $1}')"
  "$ROOT/scripts/state.sh" set "$work" waiting
  wait_for "tx capture-pane -p -t '$sb' | grep -q '🙋 base'"
}

@test "toggle off removes every sidebar, on brings them back" {
  wait_for '[ "$(sidebars_in base:)" -eq 1 ]'
  "$ROOT/scripts/sidebar-toggle.sh" off
  [ "$(sidebars_in base:)" -eq 0 ]
  "$ROOT/scripts/sidebar-toggle.sh" on
  [ "$(sidebars_in base:)" -eq 1 ]
}

@test "a window left with only its sidebar is closed" {
  w="$(tx new-window -d -t base: -P -F '#{window_id}')"
  wait_for '[ "$(sidebars_in "$w")" -eq 1 ]'
  work="$(tx list-panes -t "$w" -F '#{pane_id} #{@agents_sidebar_pane}' | awk '$2!=1{print $1}')"
  tx kill-pane -t "$work"
  wait_for "! tx list-windows -a -F '#{window_id}' | grep -qx '$w'"
}

@test "status-right shows the agent counts" {
  [[ "$(tx show-options -gqv status-right)" == '#{@agents_status}'* ]]
}

@test "a sidebar is reloaded only when its rows change" {
  wait_for '[ "$(sidebars_in base:)" -eq 1 ]'
  sb="$(tx list-panes -t base: -F '#{pane_id} #{@agents_sidebar_pane}' | awk '$2==1{print $1}')"
  wait_for "tx capture-pane -p -t '$sb' | grep -q 'base'"
  tx pipe-pane -O -t "$sb" "cat >>'$TMPDIR/drawn'"
  sleep 0.3
  : >"$TMPDIR/drawn"
  "$ROOT/scripts/refresh.sh"
  sleep 0.5
  [ ! -s "$TMPDIR/drawn" ]
  tx new-session -d -s added
  "$ROOT/scripts/refresh.sh"
  wait_for "tx capture-pane -p -t '$sb' | grep -q 'added'"
}

@test "the footer shows the details of the row under the cursor" {
  wait_for '[ "$(sidebars_in base:)" -eq 1 ]'
  sb="$(tx list-panes -t base: -F '#{pane_id} #{@agents_sidebar_pane}' | awk '$2==1{print $1}')"
  wait_for "tx capture-pane -p -t '$sb' | grep -q 'base · no agent'"
}
