#!/usr/bin/env bats
load helper

setup() { setup_server; make_repo; }
teardown() { teardown_server; }

detail() { "$ROOT/scripts/detail.sh" "$@" | strip_ansi; }

@test "a session shows its state and age, live branch, worktree and PR" {
  wt="$TMPDIR/repos/.worktrees/app-feat"
  git -C "$REPO" worktree add -q -b feat "$wt"
  tx set-option -t =base: @agent_worktree "$wt"
  tx set-option -t =base: @agent_pr 1834
  tx set-option -t =base: @agent_pr_state open
  p="$(tx display-message -p -t base '#{pane_id}')"
  tx set-option -p -t "$p" @agent_state waiting
  tx set-option -p -t "$p" @agent_state_at "$(( $(date +%s) - 300 ))"
  run detail s:base
  [ "$status" -eq 0 ]
  [ "${lines[0]}" = "🙋 base · needs you · 5m" ]
  [ "${lines[1]}" = "⎇ feat" ]
  [ "${lines[2]}" = "📁 app-feat" ]
  [ "${lines[3]}" = "#1834 open" ]
  [ "${lines[4]}" = "n new c win o ide x close ? help" ]
  git -C "$wt" checkout -q -b other
  run detail s:base
  [ "${lines[1]}" = "⎇ other" ]
}

@test "a session outside git shows only its state and the keys" {
  tx new-window -d -t base: -n tests
  run detail s:base
  [ "${lines[0]}" = "base · no agent" ]
  [ "${lines[1]}" = "2 windows" ]
  [ "${#lines[@]}" -eq 3 ]
}

@test "a window shows its own state" {
  tx new-window -d -t base: -n tests
  tx set-option -p -t base:1 @agent_state running
  run detail w:base:1
  [ "${lines[0]}" = "⚡ base:1 tests · working" ]
}

@test "a repo shows its path and its sessions by state" {
  for s in a b; do
    tx new-session -d -s "$s" -c "$REPO"
    tx set-option -t "=$s:" @agent_repo app
    tx set-option -t "=$s:" @agent_repo_dir "$REPO"
  done
  tx set-option -p -t a @agent_state waiting
  run detail r:sessions:app
  [ "${lines[0]}" = "📂 app" ]
  [[ "${lines[1]}" == "📁 "*"/repos/app" ]]
  [ "${lines[2]}" = "2 sessions · 🙋 1" ]
}
