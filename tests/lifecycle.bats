#!/usr/bin/env bats
load helper

setup() { setup_server; make_repo; }
# The sleeps in the process test; a failed run must not leave them behind.
teardown() { pkill -f "^sleep 730[12]$" || true; teardown_server; }

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

@test "the launch command is a tmux format with the agent's name" {
  tx set-option -g @agents_launch_cmd "echo name=#{@agent_name}"
  "$ROOT/scripts/new.sh" "$REPO" feature/named --no-switch
  ai="$(tx list-panes -t =feature-named: -F '#{pane_id} #{@agents_role}' | awk '$2=="ai"{print $1}')"
  wait_for "tx capture-pane -p -t '$ai' | grep -q '^name=app-feature-named$'"
}

@test "a second agent in the same task gets its own name" {
  new feature/twice
  w="$(tx new-window -d -t =feature-twice: -P -F '#{window_id}')"
  "$ROOT/scripts/layout.sh" "$w" --no-agent
  names="$(tx list-panes -s -t =feature-twice -F '#{@agent_name}' | grep . | sort | tr '\n' ' ')"
  [ "$names" = "app-feature-twice app-feature-twice-2 " ]
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

@test "new explains a branch that is checked out in the main checkout" {
  git -C "$REPO" switch -q -c feature/here
  run new feature/here
  [ "$status" -ne 0 ]
  grep -q "feature/here is checked out in the main checkout" "$TMPDIR/tmux-agents-$(id -u)/agents.log"
  ! tx has-session -t =feature-here 2>/dev/null
}

@test "new shows why git could not create the worktree" {
  run new 'bad..name'
  [ "$status" -ne 0 ]
  grep -q "could not create a worktree for bad..name: .*bad..name" "$TMPDIR/tmux-agents-$(id -u)/agents.log"
}

@test "close --force removes an unmerged worktree and keeps the branch" {
  new feature/force
  wt="$(tx show-options -t =feature-force: -qv @agent_worktree)"
  [ -n "$wt" ] && [ -d "$wt" ]  # never let git -C "" touch this repo
  git -C "$wt" -c user.email=t@t -c user.name=t commit -q --allow-empty -m work
  run "$ROOT/scripts/close.sh" --force feature-force
  [ "$status" -eq 0 ]
  ! tx has-session -t =feature-force
  [ ! -d "$wt" ]
  git -C "$REPO" show-ref --verify -q refs/heads/feature/force
}

@test "close --force still refuses uncommitted changes" {
  new feature/force-dirty
  wt="$(tx show-options -t =feature-force-dirty: -qv @agent_worktree)"
  [ -n "$wt" ] && [ -d "$wt" ]  # never let git -C "" touch this repo
  touch "$wt/new-file"
  run "$ROOT/scripts/close.sh" --force feature-force-dirty
  [ "$status" -eq 1 ]
  tx has-session -t =feature-force-dirty
  [ -d "$wt" ]
}

@test "close stops processes left in the worktree, also orphans, and no others" {
  new feature/procs
  wt="$(tx show-options -t =feature-procs: -qv @agent_worktree)"
  [ -n "$wt" ] && [ -d "$wt" ]  # never let git -C "" touch this repo
  mkdir -p "$wt/sub"
  # The subshell exits, so each sleep is an orphan like a dev server left by mise.
  (cd "$wt/sub" && sleep 7301 >/dev/null 2>&1 3>&- &)
  (cd "$TMPDIR" && sleep 7302 >/dev/null 2>&1 3>&- &)
  wait_for 'pgrep -f "^sleep 7301$" >/dev/null'
  run "$ROOT/scripts/close.sh" --keep feature-procs
  [ "$status" -eq 0 ]
  wait_for '! pgrep -f "^sleep 7301$" >/dev/null'
  pgrep -f "^sleep 7302$" >/dev/null
}

@test "close runs docker compose down for projects started in the worktree" {
  new feature/compose
  wt="$(tx show-options -t =feature-compose: -qv @agent_worktree)"
  mkdir -p "$TMPDIR/bin"
  cat >"$TMPDIR/bin/docker" <<FAKE
#!/bin/sh
case "\$1" in
  ps) printf 'app-wt\t%s/services/web\nother\t/elsewhere\n' "$wt" ;;
  compose) echo "\$*" >>"$TMPDIR/docker.log" ;;
esac
FAKE
  chmod +x "$TMPDIR/bin/docker"
  PATH="$TMPDIR/bin:$PATH" run "$ROOT/scripts/close.sh" --keep feature-compose
  [ "$status" -eq 0 ]
  [ "$(cat "$TMPDIR/docker.log")" = "compose -p app-wt down" ]
}
