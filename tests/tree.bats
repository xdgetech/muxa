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
  [ "${lines[0]}" = "r:app	 app" ]
  [[ "${lines[1]}" == "s:a-task	├ "*"a-task" ]]
  [[ "${lines[2]}" == "s:b-task	└ "*"b-task" ]]
  [ "${lines[3]}" = "r:~	 sessions" ]
  [[ "${lines[4]}" == "s:base	└ "*"base" ]]
}

@test "a session shows its most urgent state and its PR" {
  tx set-option -t =base: @agent_pr 1834
  tx split-window -t base
  p1="$(tx list-panes -t base -F '#{pane_id}' | sed -n 1p)"
  p2="$(tx list-panes -t base -F '#{pane_id}' | sed -n 2p)"
  tx set-option -p -t "$p1" @agent_state running
  tx set-option -p -t "$p2" @agent_state waiting
  run tree
  [[ "${lines[1]}" == "s:base	└ ◐ base #1834" ]]
}

@test "merged PR shows a check unless the agent is busy" {
  tx set-option -t =base: @agent_pr_state merged
  run tree
  [[ "${lines[1]}" == *"✓ base" ]]
}

@test "windows are listed only when a session has more than one" {
  run tree
  [ "${#lines[@]}" -eq 2 ]
  tx new-window -d -t base: -n tests
  run tree
  [ "${#lines[@]}" -eq 4 ]
  [[ "${lines[2]}" == "w:base:0	  ├ "* ]]
  [[ "${lines[3]}" == "w:base:1	  └ "*"1:tests" ]]
}

@test "sidebar panes do not count toward a session's state" {
  p="$(tx display-message -p -t base '#{pane_id}')"
  tx set-option -p -t "$p" @agents_sidebar_pane 1
  tx set-option -p -t "$p" @agent_state waiting
  run tree
  [[ "${lines[1]}" != *"◐"* ]]
}
