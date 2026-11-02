#!/usr/bin/env bats
# Tasks through muxa: the @muxa_task link at creation, the Inbox rows, and new
# tasks through muxa-cli. A fake muxa-cli stands in, so muxa need not be installed.
load helper

setup() { setup_server; make_repo; }
teardown() { kill "${CLIENT_PID:-}" 2>/dev/null || true; teardown_server; }

# Loads the plugin and attaches a real client, $C, to base.
attach() {
  load_plugin
  python3 "$BATS_TEST_DIRNAME/pty-client.py" "$TMUX_AGENTS_SOCKET" base 160 40 30 3>&- &
  CLIENT_PID=$!
  wait_for '[ -n "$(tx list-clients -F "#{client_name}")" ]'
  C="$(tx list-clients -F '#{client_name}')"
}

# Types one answer into the popup form.
answer() {
  [ -z "$1" ] || tx send-keys -K -c "$C" -l "$1"
  tx send-keys -K -c "$C" Enter
  sleep 0.3
}

new() { "$ROOT/scripts/new.sh" "$REPO" "$@" --no-agent --no-switch; }
muxa() { "$ROOT/scripts/muxa.sh" "$@"; }

# A muxa-cli that logs its arguments, prints the item in bin/out, then sleeps
# $FAKE_SLEEP like the real one waiting for the start prompt, and writes bin/ended. With FAKE_FAIL it fails instead.
fake_cli() {
  mkdir -p "$TMPDIR/bin"
  cat >"$TMPDIR/bin/muxa-cli" <<'FAKE'
#!/bin/sh
printf '%s\n' "$@" >"$(dirname "$0")/args"
if [ -n "${FAKE_FAIL:-}" ]; then echo "muxa-cli: $FAKE_FAIL" >&2; exit 1; fi
cat "$(dirname "$0")/out"
sleep "${FAKE_SLEEP:-0}"
touch "$(dirname "$0")/ended"
FAKE
  chmod +x "$TMPDIR/bin/muxa-cli"
  printf '%s\n' "$1" >"$TMPDIR/bin/out"
  tx set-option -g @agents_muxa_cli "$TMPDIR/bin/muxa-cli"
  tx set-option -g @agents_muxa auto
}

INBOX='{"workspaces":["work"],"truncated":false,"tasks":[
 {"group":"needs-attention","record":{"id":"t-000001","title":"Fix login","session":"feat-login"},"session":{"id":"$7","name":"feat-login"},"agent":{"state":"waiting","since":1}},
 {"group":"running","record":{"id":"t-000002","title":"Draft plan"},"session":{"id":"$8","name":"draft-plan-t-000002"},"agent":{"state":"running"}},
 {"group":"running","record":{"id":"t-000003","title":"Second"},"session":{"id":"$9","name":"second"},"agent":{"state":"running"}},
 {"group":"no-agent","record":{"id":"t-000004","title":"Old one","session":"gone"}}]}'

@test "the plugin says new.sh takes --muxa-task" {
  load_plugin
  [ "$(tx show-options -gqv @agents_features)" = muxa-task ]
}

@test "--muxa-task tags the session at creation and keeps the branch's name" {
  run new feature/linked --muxa-task t-7f3a2c
  [ "$status" -eq 0 ]
  [ "$output" = feature-linked ]
  [ "$(tx show-options -t =feature-linked: -qv @muxa_task)" = t-7f3a2c ]
  [ -z "$(tx show-options -t =feature-linked: -qv @agent_task)" ]
}

@test "--muxa-task never takes a session from another task" {
  new feature/shared --muxa-task t-000001
  new feature/shared --muxa-task t-000002
  [ "$(tx show-options -t =feature-shared: -qv @muxa_task)" = t-000001 ]
  new feature/plain
  new feature/plain --muxa-task t-000003
  [ "$(tx show-options -t =feature-plain: -qv @muxa_task)" = t-000003 ]
}

@test "--muxa-task refuses an id that is not one" {
  run new feature/bad --muxa-task 'x; rm'
  [ "$status" -ne 0 ]
  ! tx has-session -t =feature-bad 2>/dev/null
}

@test "muxa is unavailable without muxa-cli, or when turned off" {
  tx set-option -g @agents_muxa auto
  tx set-option -g @agents_muxa_cli "$TMPDIR/none/muxa-cli"
  run muxa available
  [ "$status" -ne 0 ]
  fake_cli '{}'
  muxa available
  tx set-option -g @agents_muxa off
  run muxa available
  [ "$status" -ne 0 ]
}

@test "the Inbox lists tasks in muxa's group order, with each task's session id" {
  run bash -c "printf '%s' '$INBOX' | '$ROOT/scripts/muxa.sh' rows | sed \$'s/\x1b\\\\[[0-9;]*m//g'"
  [ "$status" -eq 0 ]
  expected="$(printf '%s\n' \
    $'$7\tNeeds attention   🙋 Fix login  t-000001  feat-login' \
    $'$8\tRunning           ⚡ Draft plan  t-000002  draft-plan-t-000002' \
    $'$9\t                  ⚡ Second  t-000003  second' \
    $'\tNo agent             Old one  t-000004  no session')"
  [ "$output" = "$expected" ]
}

@test "tasks asks muxa-cli for JSON from a bridge on this server" {
  fake_cli "$INBOX"
  run muxa tasks
  [ "$status" -eq 0 ]
  [ "$(printf '%s' "$output" | jq -r '.tasks | length')" = 4 ]
  [ "$(cat "$TMPDIR/bin/args")" = "$(printf '%s\n' --bridge "muxa-bridge --socket $TMUX_AGENTS_SOCKET" tasks --json)" ]
}

@test "create starts a muxa task and returns its session before muxa-cli ends" {
  fake_cli '{"group":"no-agent","record":{"id":"t-00000a"},"session":{"id":"$4","name":"feature-x"}}'
  touch "$TMPDIR/bin/muxa-bridge" && chmod +x "$TMPDIR/bin/muxa-bridge"
  # Into a file: bats keeps its own pipes open in every child.
  FAKE_SLEEP=3 muxa create work "$REPO" feature/x "Fix it" "" >"$TMPDIR/sid" 3>&-
  [ "$(cat "$TMPDIR/sid")" = '$4' ]
  [ ! -e "$TMPDIR/bin/ended" ]
  wait_for '[ -e "$TMPDIR/bin/ended" ]'
  [ "$(cat "$TMPDIR/bin/args")" = "$(printf '%s\n' --bridge "$TMPDIR/bin/muxa-bridge --socket $TMUX_AGENTS_SOCKET" \
    task new --workspace work --title "Fix it" --goal "Fix it" --json --repo "$REPO" --branch feature/x)" ]
}

@test "create without a repo asks for a task in the workspace" {
  fake_cli '{"group":"no-agent","record":{"id":"t-00000b"},"session":{"id":"$5","name":"x"}}'
  run muxa create work "" "" "Write the memo" "For the board"
  [ "$output" = '$5' ]
  [ "$(cat "$TMPDIR/bin/args")" = "$(printf '%s\n' --bridge "muxa-bridge --socket $TMUX_AGENTS_SOCKET" \
    task new --workspace work --title "Write the memo" --goal "For the board" --json)" ]
}

@test "create says why muxa refused" {
  fake_cli ''
  FAKE_FAIL="bad-request: no workspace named \"nope\"" run muxa create nope "" "" T ""
  [ "$status" -ne 0 ]
  [[ "$output" == *'muxa could not start the task: bad-request: no workspace named "nope"'* ]]
}

@test "prefix+T shows the Inbox, and Enter goes to the task's session" {
  new feature/inbox >/dev/null
  sid="$(tx display-message -p -t =feature-inbox: '#{session_id}')"
  fake_cli "$(printf '{"workspaces":["work"],"truncated":false,"tasks":[{"group":"running","record":{"id":"t-0000c1","title":"Inbox task"},"session":{"id":"%s","name":"feature-inbox"},"agent":{"state":"running"}}]}' "$sid")"
  attach
  tx send-keys -K -c "$C" C-b T
  sleep 1.5
  tx send-keys -K -c "$C" Enter
  wait_for '[ "$(tx display-message -p -c "$C" "#{session_name}")" = feature-inbox ]'
}

@test "prefix+N asks for a muxa task, starts it through muxa-cli, and goes there" {
  new feature/form >/dev/null
  sid="$(tx display-message -p -t =feature-form: '#{session_id}')"
  fake_cli "$(printf '{"workspaces":["home","work"],"tasks":[],"group":"no-agent","record":{"id":"t-0000d1"},"session":{"id":"%s","name":"feature-form"}}' "$sid")"
  tx set-option -g @agents_muxa_workspace work
  attach
  tx send-keys -K -c "$C" C-b N
  sleep 1.5
  answer ""              # workspace: the default, work
  answer "$REPO"
  answer feature/form    # branch
  answer "Form task"     # title
  answer ""              # goal: the title
  wait_for '[ "$(tx display-message -p -c "$C" "#{session_name}")" = feature-form ]'
  [ "$(cat "$TMPDIR/bin/args")" = "$(printf '%s\n' --bridge "muxa-bridge --socket $TMUX_AGENTS_SOCKET" \
    task new --workspace work --title "Form task" --goal "Form task" --json --repo "$REPO" --branch feature/form)" ]
}

@test "the Inbox renders CLI needs without inferring menus or unknown actions" {
  result="$(printf '%s' "$INBOX" | jq '.tasks[0].need = "answer" | .tasks[1].need = "none" | .tasks[2].need = "future" | .tasks[3].need = "no-session"' | muxa rows)"
  [[ "$result" == *'waits for your answer: Enter to open'* ]]
  [[ "$result" == *'no session'* ]]
  [[ "$result" != *'future'* ]]
  [ "$(printf '%s\n' "$result" | head -n 1 | cut -f1)" = '$7' ]
  [ "$(printf '%s\n' "$result" | wc -l | tr -d ' ')" = 4 ]
}
