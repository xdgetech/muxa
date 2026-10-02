#!/usr/bin/env bats
load helper

setup() { setup_server; make_repo; }
teardown() { teardown_server; }

new() { "$ROOT/scripts/new.sh" "$REPO" "$@" --no-agent --no-switch; }

@test "new creates a worktree, a session, and AI | terminal panes" {
  run new feature/retry
  [ "$status" -eq 0 ]
  [ "$output" = feature-retry ]
  wt="$(tx show-options -t =feature-retry: -qv @agent_worktree)"
  [ "$wt" = "$(cd "$TMPDIR" && pwd -P)/repos/.worktrees/app-feature-retry" ]
  [ "$(git -C "$wt" branch --show-current)" = feature/retry ]
  [ "$(tx show-options -t =feature-retry: -qv @agent_repo)" = app ]
  roles="$(tx list-panes -t =feature-retry: -F '#{@agents_role}' | sort | tr '\n' ' ')"
  [ "$roles" = "ai term " ]
  for p in $(tx list-panes -t =feature-retry: -F '#{pane_id}'); do
    [ "$(pane_opt "$p" pane_current_path)" = "$wt" ]
  done
}

@test "new reuses the session and worktree for the same branch" {
  new feature/retry
  run new feature/retry
  [ "$status" -eq 0 ]
  [ "$(tx list-sessions -F '#{session_name}' | grep -c '^feature-retry$')" -eq 1 ]
  [ "$(git -C "$REPO" worktree list | wc -l | tr -d ' ')" -eq 2 ]
}

@test "new uses the task id as the session name" {
  run new agent/AGT-1 --task AGT-1
  [ "$output" = AGT-1 ]
  [ "$(tx show-options -t =AGT-1: -qv @agent_task)" = AGT-1 ]
}

@test "new types the launch command into the AI pane" {
  tx set-option -g @agents_launch_cmd "echo agent-started"
  "$ROOT/scripts/new.sh" "$REPO" feature/launch --no-switch
  ai="$(tx list-panes -t =feature-launch: -F '#{pane_id} #{@agents_role}' | awk '$2=="ai"{print $1}')"
  wait_for "tx capture-pane -p -t '$ai' | grep -q '^agent-started'"
}

@test "close refuses a worktree with uncommitted changes" {
  new feature/dirty
  wt="$(tx show-options -t =feature-dirty: -qv @agent_worktree)"
  [ -n "$wt" ] && [ -d "$wt" ]  # never let git -C "" touch this repo
  touch "$wt/new-file"
  run "$ROOT/scripts/close.sh" feature-dirty
  [ "$status" -eq 1 ]
  tx has-session -t =feature-dirty
  [ -d "$wt" ]
}

@test "close refuses an unmerged branch" {
  new feature/work
  wt="$(tx show-options -t =feature-work: -qv @agent_worktree)"
  [ -n "$wt" ] && [ -d "$wt" ]  # never let git -C "" touch this repo
  git -C "$wt" -c user.email=t@t -c user.name=t commit -q --allow-empty -m work
  run "$ROOT/scripts/close.sh" feature-work
  [ "$status" -eq 1 ]
  [ -d "$wt" ]
}

@test "close removes a clean merged worktree and keeps the branch" {
  new feature/done
  wt="$(tx show-options -t =feature-done: -qv @agent_worktree)"
  run "$ROOT/scripts/close.sh" feature-done
  [ "$status" -eq 0 ]
  ! tx has-session -t =feature-done
  [ ! -d "$wt" ]
  git -C "$REPO" show-ref --verify -q refs/heads/feature/done
}

@test "close --keep leaves the worktree" {
  new feature/keep
  wt="$(tx show-options -t =feature-keep: -qv @agent_worktree)"
  [ -n "$wt" ] && [ -d "$wt" ]  # never let git -C "" touch this repo
  git -C "$wt" -c user.email=t@t -c user.name=t commit -q --allow-empty -m work
  run "$ROOT/scripts/close.sh" --keep feature-keep
  [ "$status" -eq 0 ]
  ! tx has-session -t =feature-keep
  [ -d "$wt" ]
}

@test "a custom worktree command owns worktree creation" {
  custom="$TMPDIR/custom-wt"
  mkdir -p "$custom"
  printf '#!/bin/sh\necho "args: $*"\necho %s\n' "$custom" >"$TMPDIR/wt-cmd"
  chmod +x "$TMPDIR/wt-cmd"
  tx set-option -g @agents_worktree_cmd "$TMPDIR/wt-cmd"
  run new feature/custom
  [ "$status" -eq 0 ]
  [ "$(tx show-options -t =feature-custom: -qv @agent_worktree)" = "$custom" ]
}
