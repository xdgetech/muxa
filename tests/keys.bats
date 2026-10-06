#!/usr/bin/env bats
# Drives the plugin through a real attached client: keys go through tmux's key table.
load helper

setup() {
  setup_server
  tx set-option -g @agents_launch_cmd "echo agent-started"
  load_plugin
  make_repo
  "$ROOT/scripts/new.sh" "$REPO" feature/one --no-switch >/dev/null
  python3 "$BATS_TEST_DIRNAME/pty-client.py" "$TMUX_AGENTS_SOCKET" base 160 40 60 3>&- &
  CLIENT_PID=$!
  wait_for '[ -n "$(tx list-clients -F "#{client_name}")" ]'
  C="$(tx list-clients -F '#{client_name}')"
  SB="$(tx list-panes -t base: -F '#{pane_id} #{@agents_sidebar_pane}' | awk '$2 == 1 { print $1 }')"
  wait_for "tx capture-pane -p -t '$SB' | grep -q feature-one"
}
teardown() { kill "$CLIENT_PID" 2>/dev/null || true; teardown_server; }

key() { tx send-keys -K -c "$C" "$@"; sleep 0.5; }

# Wait until the sidebar cursor is on a row, by the text it shows. The cursor
# row is the one with the grey (236) background.
select_row() { wait_for "tx capture-pane -e -p -t '$SB' | grep -q '48;5;236m.*$1'" ; }
active() { tx display-message -p -c "$C" "#{$1}"; }

@test "prefix+Tab moves into the sidebar and back" {
  [ -z "$(active @agents_sidebar_pane)" ]
  key C-b Tab
  [ "$(active @agents_sidebar_pane)" = 1 ]
  key C-b Tab
  [ -z "$(active @agents_sidebar_pane)" ]
}

@test "Enter on a task switches to its AI pane, and the old window keeps its work pane" {
  key C-b Tab
  key k
  key k
  select_row feature-one
  key Enter
  [ "$(active session_name)" = feature-one ]
  [ "$(active @agents_role)" = ai ]
  [ -z "$(tx display-message -p -t base: '#{@agents_sidebar_pane}')" ]
}

@test "Esc leaves the sidebar without switching" {
  key C-b Tab
  key Escape
  [ "$(active session_name)" = base ]
  [ -z "$(active @agents_sidebar_pane)" ]
}

@test "n asks for repo and branch, then creates and enters the task" {
  key C-b Tab
  key k
  key k
  select_row feature-one
  key n
  key Enter
  tx send-keys -K -c "$C" -l feature/two
  key Enter
  wait_for '[ "$(active session_name)" = feature-two ]'
  [ "$(active @agents_role)" = ai ]
}

@test "x then y closes a merged task and removes its worktree" {
  wt="$(tx show-options -t =feature-one: -qv @agent_worktree)"
  key C-b Tab
  key k
  key k
  select_row feature-one
  key x
  sleep 0.5
  key y
  wait_for '! tx has-session -t =feature-one 2>/dev/null'
  [ ! -d "$wt" ]
}

@test "a finished turn stays done until you look at its window" {
  ai="$(tx list-panes -t feature-one: -F '#{pane_id} #{@agents_role}' | awk '$2 == "ai" { print $1 }')"
  "$ROOT/scripts/state.sh" set "$ai" done
  [ "$(pane_opt "$ai" @agent_state)" = done ]
  tx switch-client -c "$C" -t =feature-one
  wait_for "[ \"\$(pane_opt '$ai' @agent_state)\" = idle ]"
}

@test "a turn that finishes while you watch is idle at once" {
  p="$(tx list-panes -t base: -F '#{pane_id} #{@agents_sidebar_pane}' | awk '$2 != 1 { print $1 }')"
  "$ROOT/scripts/state.sh" set "$p" running
  "$ROOT/scripts/state.sh" set "$p" done
  [ "$(pane_opt "$p" @agent_state)" = idle ]
}
