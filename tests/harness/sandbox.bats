#!/usr/bin/env bats
# The sandbox builds an isolated agent | shell layout, and layout-snapshot
# records it stably.

ROOT="$BATS_TEST_DIRNAME/../.."

setup() {
  export MUXA_SOCKET="muxa-test-$$-$BATS_TEST_NUMBER"
  unset TMUX TMUX_PANE
}

teardown() {
  "$ROOT/scripts/tmux-sandbox.sh" down
}

@test "up creates two sessions with an agent and a shell pane each" {
  run "$ROOT/scripts/tmux-sandbox.sh" up
  [ "$status" -eq 0 ]
  run tmux -L "$MUXA_SOCKET" list-panes -a -F '#{session_name} #{@muxa_role}'
  [ "$status" -eq 0 ]
  [ "$(printf '%s\n' "$output" | sort | tr '\n' ' ')" = "demo-a agent demo-a shell demo-b agent demo-b shell " ]
}

@test "up refuses to run twice" {
  "$ROOT/scripts/tmux-sandbox.sh" up
  run "$ROOT/scripts/tmux-sandbox.sh" up
  [ "$status" -eq 1 ]
}

@test "layout-snapshot diffs to empty when nothing changed" {
  "$ROOT/scripts/tmux-sandbox.sh" up
  "$ROOT/scripts/layout-snapshot.sh" record -L "$MUXA_SOCKET" >"$BATS_TEST_TMPDIR/before"
  "$ROOT/scripts/layout-snapshot.sh" record -L "$MUXA_SOCKET" >"$BATS_TEST_TMPDIR/after"
  run "$ROOT/scripts/layout-snapshot.sh" diff "$BATS_TEST_TMPDIR/before" "$BATS_TEST_TMPDIR/after"
  [ "$status" -eq 0 ]
  [ "$(wc -l <"$BATS_TEST_TMPDIR/before")" -eq 4 ]
}

@test "layout-snapshot reports a resized pane" {
  "$ROOT/scripts/tmux-sandbox.sh" up
  "$ROOT/scripts/layout-snapshot.sh" record -L "$MUXA_SOCKET" >"$BATS_TEST_TMPDIR/before"
  pane="$(tmux -L "$MUXA_SOCKET" list-panes -a -F '#{pane_id}' | head -n 1)"
  tmux -L "$MUXA_SOCKET" resize-pane -t "$pane" -x 50
  "$ROOT/scripts/layout-snapshot.sh" record -L "$MUXA_SOCKET" >"$BATS_TEST_TMPDIR/after"
  run "$ROOT/scripts/layout-snapshot.sh" diff "$BATS_TEST_TMPDIR/before" "$BATS_TEST_TMPDIR/after"
  [ "$status" -eq 1 ]
}

@test "down stops the server" {
  "$ROOT/scripts/tmux-sandbox.sh" up
  "$ROOT/scripts/tmux-sandbox.sh" down
  run tmux -L "$MUXA_SOCKET" has-session
  [ "$status" -ne 0 ]
}
