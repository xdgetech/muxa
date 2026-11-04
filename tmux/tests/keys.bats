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
  wait_for '[ ! -d "$wt" ]'  # removed after the session is gone
}

@test "ctrl-x then y removes the worktree of an unmerged task" {
  wt="$(tx show-options -t =feature-one: -qv @agent_worktree)"
  [ -n "$wt" ] && [ -d "$wt" ]  # never let git -C "" touch this repo
  git -C "$wt" -c user.email=t@t -c user.name=t commit -q --allow-empty -m work
  key C-b Tab
  key k
  key k
  select_row feature-one
  key C-x
  sleep 0.5
  key y
  wait_for '! tx has-session -t =feature-one 2>/dev/null'
  wait_for '[ ! -d "$wt" ]'  # removed after the session is gone
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

# The tree is: app / 1 feature-one, then sessions / 2 base.
@test "prefix+) and prefix+( go to the next and previous task in tree order" {
  key C-b ')'
  wait_for '[ "$(active session_name)" = feature-one ]'
  [ "$(active @agents_role)" = ai ]
  key C-b '('
  wait_for '[ "$(active session_name)" = base ]'
}

@test "prefix+j then a number goes to that task" {
  key C-b j 1
  wait_for '[ "$(active session_name)" = feature-one ]'
  key C-b j 2
  wait_for '[ "$(active session_name)" = base ]'
}

@test "prefix+j then a letter goes to the 10th task and on" {
  for i in 1 2 3 4 5 6 7 8; do tx new-session -d -s "x$i"; done
  key C-b j a
  wait_for '[ "$(active session_name)" = x8 ]'
}

@test "a number in the sidebar goes to that task" {
  key C-b Tab
  key 1
  wait_for '[ "$(active session_name)" = feature-one ]'
  [ -z "$(active @agents_sidebar_pane)" ]
}

@test "prefix+a goes to the task that needs you" {
  ai="$(tx list-panes -t feature-one: -F '#{pane_id} #{@agents_role}' | awk '$2 == "ai" { print $1 }')"
  "$ROOT/scripts/state.sh" set "$ai" waiting
  key C-b a
  wait_for '[ "$(active session_name)" = feature-one ]'
}
