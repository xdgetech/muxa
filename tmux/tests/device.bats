#!/usr/bin/env bats
# muxa's device sessions (@muxa_device) hold one window of a session while a
# phone or tablet shows it. The plugin lists every window once, and closing a
# task closes them too.
load helper

setup() { setup_server; }
teardown() { teardown_server; }

# A device session as muxa makes it: its own session, holding window $1.
make_device() {
  local out
  out="$(tx new-session -d -P -F '#{session_id} #{window_id}' cat)"
  DEVICE="${out% *}"
  tx set-option -t "$DEVICE" @muxa_device 1 \; link-window -d -s "$1" -t "$DEVICE:" \; kill-window -t "${out#* }"
}

@test "the tree, the status line and the jump order skip device sessions" {
  pane="$(tx display-message -p -t base: '#{pane_id}')"
  tx set-option -p -t "$pane" @agent_state waiting
  make_device "$(tx display-message -p -t base: '#{window_id}')"
  [ "$(tx list-sessions | wc -l | tr -d ' ')" = 2 ]
  run "$ROOT/scripts/tree.sh"
  [ "$status" -eq 0 ]
  [ "$(printf '%s\n' "$output" | grep -c '^s:')" = 1 ]
  "$ROOT/scripts/status.sh"
  [ "$(tx show-options -gqv @agents_status)" = "🙋 1 " ]
}

@test "close also closes the device sessions that hold its windows" {
  make_repo
  "$ROOT/scripts/new.sh" "$REPO" feature/shown --no-agent --no-switch
  window="$(tx display-message -p -t =feature-shown: '#{window_id}')"
  make_device "$window"
  wt="$(tx show-options -t =feature-shown: -qv @agent_worktree)"
  [ -n "$wt" ] && [ -d "$wt" ]  # never let git -C "" touch this repo
  git -C "$wt" -c user.email=t@t -c user.name=t commit -q --allow-empty -m work
  run "$ROOT/scripts/close.sh" --keep feature-shown
  [ "$status" -eq 0 ]
  ! tx has-session -t "$DEVICE"
  ! tx list-windows -a -F '#{window_id}' | grep -qx "$window"
}
