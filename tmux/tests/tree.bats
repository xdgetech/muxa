#!/usr/bin/env bats
load helper

setup() { setup_server; }
teardown() { teardown_server; }

tree() { "$ROOT/scripts/tree.sh" "$@" | strip_ansi; }

@test "sessions are grouped by repo, sessions without one last" {
  tx new-session -d -s b-task
  tx set-option -t =b-task: @agent_repo app
  tx new-session -d -s a-task
  tx set-option -t =a-task: @agent_repo app
  run tree
  [ "$status" -eq 0 ]
  [ "${lines[0]}" = "g:sessions	▾ Sessions 3" ]
  [ "${lines[1]}" = "r:sessions:app	  📂 app" ]
  [ "${lines[2]}" = "s:a-task	 1    a-task" ]
  [ "${lines[3]}" = "s:b-task	 2    b-task" ]
  [ "${lines[4]}" = "r:sessions:~	  no repo" ]
  [ "${lines[5]}" = "s:base	 3    base" ]
}

@test "Tasks, Routine runs, ask sessions and other sessions are sections" {
  mkdir -p "$BATS_TEST_TMPDIR/w/tasks/fix-t-000001" "$BATS_TEST_TMPDIR/w/routines/brief-t-000002"
  tx new-session -d -s fix -c "$BATS_TEST_TMPDIR/w/tasks/fix-t-000001"
  tx set-option -t =fix: @muxa_task t-000001
  tx new-session -d -s feat
  tx set-option -t =feat: @muxa_task t-000003
  tx set-option -t =feat: @agent_repo app
  tx new-session -d -s brief -c "$BATS_TEST_TMPDIR/w/routines/brief-t-000002"
  tx set-option -t =brief: @muxa_task t-000002
  tx new-session -d -s ask-app-1
  tx set-option -t =ask-app-1: @muxa_ask work
  run tree
  [ "$status" -eq 0 ]
  [ "${lines[0]}" = "g:tasks	▾ Tasks 2" ]
  [ "${lines[1]}" = "r:tasks:app	  📂 app" ]
  [ "${lines[2]}" = "s:feat	 1    feat" ]
  [ "${lines[3]}" = "r:tasks:~	  no repo" ]
  [ "${lines[4]}" = "s:fix	 2    fix" ]
  [ "${lines[5]}" = "g:routines	▾ Routines 1" ]
  [ "${lines[6]}" = "s:brief	 3    brief" ]
  [ "${lines[7]}" = "g:ask	▾ Ask 1" ]
  [ "${lines[8]}" = "s:ask-app-1	 4    ask-app-1" ]
  [ "${lines[9]}" = "g:sessions	▾ Sessions 1" ]
  [ "${lines[10]}" = "s:base	 5    base" ]
}

@test "an assistant run is never a Task: it lists under Assistant (ADR 0046)" {
  mkdir -p "$BATS_TEST_TMPDIR/w/tasks/fix-t-000001" "$BATS_TEST_TMPDIR/w/assistant/hosting-t-000004"
  tx new-session -d -s fix -c "$BATS_TEST_TMPDIR/w/tasks/fix-t-000001"
  tx set-option -t =fix: @muxa_task t-000001
  tx new-session -d -s hosting -c "$BATS_TEST_TMPDIR/w/assistant/hosting-t-000004"
  tx set-option -t =hosting: @muxa_task t-000004
  run tree
  [ "$status" -eq 0 ]
  [ "${lines[0]}" = "g:tasks	▾ Tasks 1" ]
  [ "${lines[1]}" = "s:fix	 1    fix" ]
  [ "${lines[2]}" = "g:assistant	▾ Assistant 1" ]
  [ "${lines[3]}" = "s:hosting	 2    hosting" ]
  [ "${lines[4]}" = "g:sessions	▾ Sessions 1" ]
  [ "${lines[5]}" = "s:base	 3    base" ]
}

@test "a folded section hides its sessions and their labels, and shows their state" {
  tx new-session -d -s ask-app-1
  tx set-option -t =ask-app-1: @muxa_ask work
  tx set-option -p -t ask-app-1 @agent_state waiting
  tx set-option -g @agents_folded ask
  run tree
  [ "${lines[0]}" = "g:ask	▸ Ask 1 🙋" ]
  [ "${lines[1]}" = "g:sessions	▾ Sessions 1" ]
  [ "${lines[2]}" = "s:base	 1    base" ]
  run tree --all
  [ "${lines[1]}" = "s:ask-app-1	 1 🙋 ask-app-1" ]
}

@test "a folded section marks the sidebar's own session" {
  tx set-option -g @agents_folded sessions
  run tree "$(tx display-message -p -t base:0 '#{pane_id}')"
  [ "${lines[0]}" = "g:sessions	▎▸ Sessions 1" ]
  [ "${#lines[@]}" -eq 1 ]
}

@test "a session shows its most urgent state and its PR" {
  tx set-option -t =base: @agent_pr 1834
  tx split-window -t base
  p1="$(tx list-panes -t base -F '#{pane_id}' | sed -n 1p)"
  p2="$(tx list-panes -t base -F '#{pane_id}' | sed -n 2p)"
  tx set-option -p -t "$p1" @agent_state running
  tx set-option -p -t "$p2" @agent_state waiting
  run tree
  [ "${lines[1]}" = "s:base	 1 🙋 base #1834" ]
}

# The check used to replace the state icon; now both show.
@test "a merged PR shows a check next to the state" {
  tx set-option -t =base: @agent_pr 1834
  tx set-option -t =base: @agent_pr_state merged
  tx set-option -p -t base @agent_state running
  run tree
  [ "${lines[1]}" = "s:base	 1 ⚡ base ✅" ]
}

@test "state icons come from the options" {
  tx set-option -g @agents_icon_idle zz
  tx set-option -p -t base @agent_state idle
  run tree
  [ "${lines[1]}" = "s:base	 1 zz base" ]
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
  [ "${lines[1]}" = "s:base	▎1    base" ]
  [[ "${lines[2]}" == "w:base:0	▎       0 "* ]]
  [ "${lines[3]}" = "w:base:1	▎       1 tests" ]
  [ "${lines[4]}" = "s:other	 2    other" ]
}

@test "sidebar panes do not count toward a session's state" {
  p="$(tx display-message -p -t base '#{pane_id}')"
  tx set-option -p -t "$p" @agents_sidebar_pane 1
  tx set-option -p -t "$p" @agent_state waiting
  run tree
  [[ "${lines[1]}" != *"🙋"* ]]
}

@test "sessions after the 9th are labelled with letters, up to z" {
  for i in $(seq 10 44); do tx new-session -d -s "t$i"; done
  run tree
  [ "${lines[1]}" = "s:base	 1    base" ]
  [ "${lines[9]}" = "s:t17	 9    t17" ]
  [ "${lines[10]}" = "s:t18	 a    t18" ]
  [ "${lines[35]}" = "s:t43	 z    t43" ]
  [ "${lines[36]}" = "s:t44	      t44" ]
}
