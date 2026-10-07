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

# A task window: sidebar | AI pane | terminal pane.
task_window() {
  w="$(tx new-window -d -t base: -P -F '#{window_id}')"
  wait_for '[ "$(sidebars_in "$w")" -eq 1 ]'
  "$ROOT/scripts/layout.sh" "$w" --no-agent
}

width_of() {
  tx list-panes -t "$1" -F $'#{pane_width}\t#{@agents_sidebar_pane}\t#{@agents_role}' |
    awk -F'\t' -v k="$2" '($2 == "1" ? "sidebar" : $3) == k { print $1 }'
}

# The terminal pane's share of the work area, in percent.
term_share() {
  local ai term
  ai="$(width_of "$1" ai)" term="$(width_of "$1" term)"
  echo $(( term * 100 / (ai + term) ))
}

near() { [ $(( $1 - $2 )) -le 1 ] && [ $(( $2 - $1 )) -le 1 ]; }

@test "toggling the sidebar keeps the AI | terminal split" {
  task_window
  before="$(term_share "$w")"
  "$ROOT/scripts/sidebar-toggle.sh" off
  near "$(term_share "$w")" "$before"
  "$ROOT/scripts/sidebar-toggle.sh" on
  near "$(term_share "$w")" "$before"
  [ "$(width_of "$w" sidebar)" -eq 28 ]
}

@test "resizing the window keeps the split and the sidebar width" {
  task_window
  before="$(term_share "$w")"
  tx set-option -w -t "$w" window-size manual
  tx resize-window -t "$w" -x 150
  "$ROOT/scripts/sidebar-toggle.sh" fit "$w"
  near "$(term_share "$w")" "$before"
  [ "$(width_of "$w" sidebar)" -eq 28 ]
  tx resize-window -t "$w" -x 220
  "$ROOT/scripts/sidebar-toggle.sh" fit "$w"
  near "$(term_share "$w")" "$before"
  [ "$(width_of "$w" sidebar)" -eq 28 ]
}

@test "a split resized by hand survives a toggle" {
  task_window
  term="$(tx list-panes -t "$w" -F '#{pane_id} #{@agents_role}' | awk '$2=="term"{print $1}')"
  tx resize-pane -t "$term" -x 50
  before="$(term_share "$w")"
  "$ROOT/scripts/sidebar-toggle.sh" off
  "$ROOT/scripts/sidebar-toggle.sh" on
  near "$(term_share "$w")" "$before"
}

@test "the window-resized hook fits the sidebar" {
  task_window
  before="$(term_share "$w")"
  tx set-option -w -t "$w" window-size manual
  tx resize-window -t "$w" -x 120
  tx resize-window -t "$w" -x 240
  wait_for '[ "$(width_of "$w" sidebar)" -eq 28 ]'
  wait_for 'near "$(term_share "$w")" "$before"'
}

@test "a window split by hand keeps its column widths over many toggles" {
  w="$(tx new-window -d -t base: -P -F '#{window_id}')"
  wait_for '[ "$(sidebars_in "$w")" -eq 1 ]'
  work="$(tx list-panes -t "$w" -F '#{pane_id} #{@agents_sidebar_pane}' | awk '$2!=1{print $1}')"
  right="$(tx split-window -h -d -t "$work" -l 60 -P -F '#{pane_id}')"
  before="$(pane_opt "$right" pane_width)"
  for _ in 1 2 3 4 5 6 7 8 9 10; do
    "$ROOT/scripts/sidebar-toggle.sh" toggle
  done
  [ "$(sidebars_in "$w")" -eq 1 ]
  [ "$(pane_opt "$right" pane_width)" -eq "$before" ]
  [ "$(width_of "$w" sidebar)" -eq 28 ]
}
