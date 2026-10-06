#!/usr/bin/env bats
load helper

setup() { setup_server; }
teardown() { teardown_server; }

tree() { "$ROOT/scripts/tree.sh" "$@" | strip_ansi; }

@test "sessions are grouped by repo, plain sessions last" {
  tx new-session -d -s b-task
  tx set-option -t =b-task: @agent_repo app
  tx new-session -d -s a-task
  tx set-option -t =a-task: @agent_repo app
  run tree
  [ "$status" -eq 0 ]
  [ "${lines[0]}" = "r:app	📂 app" ]
  [ "${lines[1]}" = "s:a-task	    a-task" ]
  [ "${lines[2]}" = "s:b-task	    b-task" ]
  [ "${lines[3]}" = "r:~	📂 sessions" ]
  [ "${lines[4]}" = "s:base	    base" ]
}

@test "a session shows its most urgent state and its PR" {
  tx set-option -t =base: @agent_pr 1834
  tx split-window -t base
  p1="$(tx list-panes -t base -F '#{pane_id}' | sed -n 1p)"
  p2="$(tx list-panes -t base -F '#{pane_id}' | sed -n 2p)"
  tx set-option -p -t "$p1" @agent_state running
  tx set-option -p -t "$p2" @agent_state waiting
  run tree
  [ "${lines[1]}" = "s:base	 🙋 base #1834" ]
}

# The check used to replace the state icon; now both show.
@test "a merged PR shows a check next to the state" {
  tx set-option -t =base: @agent_pr 1834
  tx set-option -t =base: @agent_pr_state merged
  tx set-option -p -t base @agent_state running
  run tree
  [ "${lines[1]}" = "s:base	 ⚡ base ✅" ]
}

@test "state icons come from the options" {
  tx set-option -g @agents_icon_idle zz
  tx set-option -p -t base @agent_state idle
  run tree
  [ "${lines[1]}" = "s:base	 zz base" ]
}

# Windows used to be listed under every session with more than one.
@test "windows are listed only under the sidebar's own session" {
  tx new-window -d -t base: -n tests
  tx new-session -d -s other
  tx new-window -d -t other: -n logs
  run tree
  [ "${#lines[@]}" -eq 3 ]
  p="$(tx display-message -p -t base:0 '#{pane_id}')"
  run tree "$p"
  [ "${#lines[@]}" -eq 5 ]
  [ "${lines[1]}" = "s:base	▎   base" ]
  [[ "${lines[2]}" == "w:base:0	▎     0 "* ]]
  [ "${lines[3]}" = "w:base:1	▎     1 tests" ]
  [ "${lines[4]}" = "s:other	    other" ]
}

@test "sidebar panes do not count toward a session's state" {
  p="$(tx display-message -p -t base '#{pane_id}')"
  tx set-option -p -t "$p" @agents_sidebar_pane 1
  tx set-option -p -t "$p" @agent_state waiting
  run tree
  [[ "${lines[1]}" != *"🙋"* ]]
}
