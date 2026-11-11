#!/usr/bin/env bats
# Tasks through muxa: the @muxa_task link at creation, the Inbox rows, and new
# tasks through muxa-cli. A fake muxa-cli stands in, so muxa need not be installed.
load helper

TAB=$'\t'

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

# A muxa-cli that logs its arguments (bin/args, and one line per call in
# bin/calls), prints bin/out-<command> (else bin/out),
# then sleeps $FAKE_SLEEP like the real one waiting for the start prompt, and
# writes bin/ended. With FAKE_FAIL it fails instead.
fake_cli() {
  mkdir -p "$TMPDIR/bin"
  cat >"$TMPDIR/bin/muxa-cli" <<'FAKE'
#!/bin/sh
printf '%s\n' "$@" >"$(dirname "$0")/args"
(shift 2; echo "$*") >>"$(dirname "$0")/calls"
if [ -n "${FAKE_FAIL:-}" ]; then echo "muxa-cli: $FAKE_FAIL" >&2; exit 1; fi
# A file, as the popup gets the server's environment: a Task end that would
# lose work, unless --force (ADR 0043).
lose="$(dirname "$0")/lose"
case " $* " in
  *" --force "*) ;;
  *" task done "* | *" task drop "*) [ -f "$lose" ] && { echo "muxa-cli: would-lose-work: $(cat "$lose")" >&2; exit 1; } ;;
esac
# A capture's text comes on stdin.
case " $* " in *" capture - "*) cat >"$(dirname "$0")/stdin" ;; esac
out="$(dirname "$0")/out-$3"
[ -f "$out" ] || out="$(dirname "$0")/out"
cat "$out"
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

@test "the Inbox lists tasks in muxa's group order, with each task's session id, id and title" {
  run bash -c "printf '%s' '$INBOX' | '$ROOT/scripts/muxa.sh' rows | sed \$'s/\x1b\\\\[[0-9;]*m//g'"
  [ "$status" -eq 0 ]
  expected="$(printf '%s\n' \
    $'$7\tt-000001\tFix login\tNeeds attention   🙋 Fix login  t-000001  feat-login' \
    $'$8\tt-000002\tDraft plan\tRunning           ⚡ Draft plan  t-000002  draft-plan-t-000002' \
    $'$9\tt-000003\tSecond\t                  ⚡ Second  t-000003  second' \
    $'\tt-000004\tOld one\tNo agent             Old one  t-000004  no session')"
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

# Workers as muxa-cli lists them: one for every workspace, one for work only.
WORKERS='{"workspaces":["home","work"],"capabilities":["task-records","work:2","workers"],"tasks":[],"workers":[
 {"name":"codex","agent":"codex","description":"Codex in the repo"},
 {"name":"reviewer","agent":"claude-code","workspaces":["work"]}]}'

@test "the agent picker lists default first, then the workers the workspace may use" {
  run bash -c "printf '%s' '$WORKERS' | '$ROOT/scripts/muxa.sh' worker-rows home"
  [ "$status" -eq 0 ]
  [ "$output" = "$(printf '%s\n' "default${TAB}default  the workspace's agent" "codex${TAB}codex  codex  Codex in the repo")" ]
  run bash -c "printf '%s' '$WORKERS' | '$ROOT/scripts/muxa.sh' worker-rows work"
  [ "${#lines[@]}" -eq 3 ]
  [ "${lines[2]}" = "reviewer${TAB}reviewer  claude-code" ]
}

@test "the agent picker has no rows without workers for the workspace" {
  run bash -c "printf '%s' '{\"workspaces\":[\"home\"],\"tasks\":[]}' | '$ROOT/scripts/muxa.sh' worker-rows home"
  [ "$status" -eq 0 ] && [ -z "$output" ]
  run bash -c "printf '%s' '{\"workers\":[{\"name\":\"r\",\"agent\":\"codex\",\"workspaces\":[]}]}' | '$ROOT/scripts/muxa.sh' worker-rows home"
  [ "$status" -eq 0 ] && [ -z "$output" ]
}

@test "create passes the picked worker" {
  fake_cli '{"group":"no-agent","record":{"id":"t-00000e"},"session":{"id":"$5","name":"x"}}'
  run muxa create work "" "" "Write the memo" "" codex
  [ "$output" = '$5' ]
  [ "$(cat "$TMPDIR/bin/args")" = "$(printf '%s\n' --bridge "muxa-bridge --socket $TMUX_AGENTS_SOCKET" \
    task new --workspace work --title "Write the memo" --goal "Write the memo" --json --worker codex)" ]
}

@test "prefix+N asks for the agent when the workspace has workers" {
  sid="$(tx display-message -p -t =base: '#{session_id}')"
  fake_cli "$(printf '%s' "$WORKERS" | jq -c --arg sid "$sid" '. + {"group":"no-agent","record":{"id":"t-0000d2"},"session":{"id":$sid,"name":"base"}}')"
  tx set-option -g @agents_muxa_workspace home
  attach
  tx send-keys -K -c "$C" C-b N
  sleep 1.5
  answer ""              # workspace: the default, home
  answer "-"             # no repo
  answer "Memo"          # title
  answer ""              # goal: the title
  answer "codex"         # agent
  wait_for '[ -e "$TMPDIR/bin/ended" ]'
  [ "$(cat "$TMPDIR/bin/args")" = "$(printf '%s\n' --bridge "muxa-bridge --socket $TMUX_AGENTS_SOCKET" \
    task new --workspace home --title Memo --goal Memo --json --worker codex)" ]
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

# A work:2 host: workspace objects with a root and the repos under it.
work2_tasks() {
  printf '{"workspaces":[{"name":"home","root":"%s/home"},{"name":"work","root":"%s/repos","repos":["%s","%s/repos/group/plan","/opt/tool"]}],"truncated":false,"tasks":[]}' \
    "$TMPDIR" "$TMPDIR" "$REPO" "$TMPDIR"
}

args_are() { [ "$(cat "$TMPDIR/bin/args")" = "$(printf '%s\n' --bridge "muxa-bridge --socket $TMUX_AGENTS_SOCKET" "$@")" ]; }

@test "the repo picker shows repos under the root by their relative path" {
  mkdir -p "$TMPDIR/repos/group/plan" && git -C "$TMPDIR/repos/group/plan" init -q
  run bash -c "printf '%s' '$(work2_tasks)' | '$ROOT/scripts/muxa.sh' repo-rows work '$TMPDIR/repos/group/plan' none"
  [ "$status" -eq 0 ]
  expected="$(printf '%s\n' \
    "$TMPDIR/repos/group/plan${TAB}group/plan" \
    "$REPO${TAB}app" \
    "/opt/tool${TAB}/opt/tool" \
    "-${TAB}- no repo")"
  [ "$output" = "$expected" ]
}

# muxa-cli's own shape: names in workspaces, details in workspaceInfo.
cli_tasks() {
  printf '{"workspaces":["home","work"],"workspaceInfo":[{"name":"home","root":"%s/home"},{"name":"work","root":"%s/repos","repos":["%s","%s/repos/group/plan","/opt/tool"]}],"capabilities":["task-records","work:2"],"truncated":false,"tasks":[]}' \
    "$TMPDIR" "$TMPDIR" "$REPO" "$TMPDIR"
}

@test "the repo picker reads workspaceInfo as muxa-cli prints it" {
  mkdir -p "$TMPDIR/repos/group/plan" && git -C "$TMPDIR/repos/group/plan" init -q
  run bash -c "printf '%s' '$(cli_tasks)' | '$ROOT/scripts/muxa.sh' repo-rows work '$TMPDIR/repos/group/plan' none"
  [ "$status" -eq 0 ]
  expected="$(printf '%s\n' \
    "$TMPDIR/repos/group/plan${TAB}group/plan" \
    "$REPO${TAB}app" \
    "/opt/tool${TAB}/opt/tool" \
    "-${TAB}- no repo")"
  [ "$output" = "$expected" ]
}

@test "the repo picker lists the current repo first, also outside the workspace" {
  other="$TMPDIR/elsewhere/lib"
  mkdir -p "$other" && git -C "$other" init -q
  run bash -c "printf '%s' '$(work2_tasks)' | '$ROOT/scripts/muxa.sh' repo-rows work '$other'"
  [ "${lines[0]}" = "$other${TAB}$other" ]
  [ "${#lines[@]}" -eq 4 ]
}

@test "create without a branch lets a work:2 host name it" {
  fake_cli '{"group":"no-agent","record":{"id":"t-00000c"},"session":{"id":"$6","name":"fix-it"}}'
  run muxa create work app "" "Fix it" ""
  [ "$output" = '$6' ]
  args_are task new --workspace work --title "Fix it" --goal "Fix it" --json --repo app
}

@test "prefix+N picks a repo from the workspace, and an empty branch passes none on work:2" {
  new feature/picked >/dev/null
  sid="$(tx display-message -p -t =feature-picked: '#{session_id}')"
  fake_cli "$(printf '{"group":"no-agent","record":{"id":"t-0000d2"},"session":{"id":"%s","name":"feature-picked"}}' "$sid")"
  work2_tasks >"$TMPDIR/bin/out-tasks"
  attach
  tx send-keys -K -c "$C" C-b N
  sleep 1.5
  answer work            # workspace
  answer app             # the picker: filters to app
  answer ""              # branch: named after the title
  answer "Picked task"   # title
  answer ""              # goal
  wait_for '[ "$(tx display-message -p -c "$C" "#{session_name}")" = feature-picked ]'
  args_are task new --workspace work --title "Picked task" --goal "Picked task" --json --repo "$REPO"
}

@test "prefix+N refuses an empty branch when muxa cannot name one" {
  fake_cli '{"workspaces":["work"],"truncated":false,"tasks":[]}'
  attach
  tx send-keys -K -c "$C" C-b N
  sleep 1.5
  answer "$REPO"         # one workspace: the picker comes first; a typed path
  answer ""              # branch
  sleep 1
  args_are tasks --json
  [ "$(tx display-message -p -c "$C" "#{session_name}")" = base ]
}

@test "prefix+A asks muxa for an ask session in the picked repo and goes there" {
  tx new-session -d -s ask-app-1
  sid="$(tx display-message -p -t =ask-app-1: '#{session_id}')"
  fake_cli "$(work2_tasks)"
  printf '{"session":"%s","name":"ask-app-1","workspace":"work"}\n' "$sid" >"$TMPDIR/bin/out-ask"
  attach
  tx send-keys -K -c "$C" C-b A
  sleep 1.5
  answer work            # workspace
  answer plan            # the picker: group/plan
  wait_for '[ "$(tx display-message -p -c "$C" "#{session_name}")" = ask-app-1 ]'
  args_are ask "$TMPDIR/repos/group/plan" --workspace work --json
}

@test "prefix+A defaults to the workspace that holds the current repo" {
  tx new-session -d -s ask-app-2
  sid="$(tx display-message -p -t =ask-app-2: '#{session_id}')"
  fake_cli "$(work2_tasks)"      # home comes first
  printf '{"session":"%s","name":"ask-app-2","workspace":"work"}\n' "$sid" >"$TMPDIR/bin/out-ask"
  tx send-keys -t base: "cd '$REPO'" Enter
  wait_for '[ "$(tx display-message -p -t base: "#{pane_current_path}")" = "$(cd "$REPO" && pwd -P)" ]'
  attach
  tx send-keys -K -c "$C" C-b A
  sleep 1.5
  answer ""              # workspace: work holds the current repo
  answer ""              # the picker: the current repo comes first
  wait_for '[ "$(tx display-message -p -c "$C" "#{session_name}")" = ask-app-2 ]'
  args_are ask "$REPO" --workspace work --json
}

# A work:15 host (ADR 0054): "secretary" is the overall assistant's workspace, with no repos.
work15_tasks() {
  printf '{"workspaces":["secretary","work"],"workspaceInfo":[{"name":"secretary","root":"%s/secretary"},{"name":"work","root":"%s/repos","repos":["%s"]}],"capabilities":["task-records","work:15"],"truncated":false,"tasks":[]}' \
    "$TMPDIR" "$TMPDIR" "$REPO"
}

@test "the ask picker offers the root Ask session only on a work:15 host" {
  run bash -c "printf '%s' '$(work15_tasks)' | '$ROOT/scripts/muxa.sh' ask-rows secretary ''"
  [ "$status" -eq 0 ]
  [ "${#lines[@]}" -eq 1 ]
  [[ "${lines[0]}" == ":root${TAB}@ the Ask session at the workspace root"* ]]
  run bash -c "printf '%s' '$(cli_tasks)' | '$ROOT/scripts/muxa.sh' ask-rows work ''"
  [[ "$output" != *":root"* ]]
}

@test "prefix+A opens the overall assistant's root Ask session and goes there" {
  tx new-session -d -s ask-secretary
  sid="$(tx display-message -p -t =ask-secretary: '#{session_id}')"
  fake_cli "$(work15_tasks)"
  printf '{"session":"%s","name":"ask-secretary","workspace":"secretary"}\n' "$sid" >"$TMPDIR/bin/out-ask"
  attach
  tx send-keys -K -c "$C" C-b A
  sleep 1.5
  answer secretary       # workspace
  answer ""              # the picker: its only row, the root Ask session
  wait_for '[ "$(tx display-message -p -c "$C" "#{session_name}")" = ask-secretary ]'
  args_are ask --root --workspace secretary --json
}

@test "prefix+A opens nothing when muxa cannot start ask sessions" {
  fake_cli '{"workspaces":["work"],"truncated":false,"tasks":[]}'
  attach
  tx send-keys -K -c "$C" C-b A
  wait_for 'grep -q "cannot start ask sessions" "$TMPDIR/tmux-agents-$(id -u)/agents.log"'
  # Not args_are: the sidebar's Inbox refresh also calls muxa-cli, in any order.
  grep -qx "tasks --json" "$TMPDIR/bin/calls"
  [ -z "$(grep "^ask " "$TMPDIR/bin/calls")" ]
  [ "$(tx display-message -p -c "$C" "#{popup_active}")" != 1 ] || false
}

@test "prefix+A without muxa says so and starts nothing" {
  attach
  tx send-keys -K -c "$C" C-b A
  wait_for 'grep -q "ask sessions need muxa" "$TMPDIR/tmux-agents-$(id -u)/agents.log"'
  [ "$(tx list-sessions | wc -l | tr -d ' ')" = 1 ]
}

@test "close-task marks a Task done or dropped through muxa-cli" {
  fake_cli '{}'
  muxa close-task t-000001 done
  muxa close-task t-000002 dropped
  [ "$(cat "$TMPDIR/bin/calls")" = "$(printf '%s\n' 'task done t-000001 --json' 'task drop t-000002 --json')" ]
}

@test "close-task refuses what is not a task id or a status, before muxa-cli" {
  fake_cli '{}'
  run muxa close-task --force done
  [ "$status" -ne 0 ]
  run muxa close-task t-000001 open
  [ "$status" -ne 0 ]
  [ ! -e "$TMPDIR/bin/calls" ]
}

@test "close-task says why muxa refused" {
  fake_cli ''
  FAKE_FAIL="conflict: the task changed" run muxa close-task t-000001 done
  [ "$status" -ne 0 ]
  [[ "$output" == *'muxa could not mark t-000001 done: conflict: the task changed'* ]]
}

@test "prefix+T: ctrl-d then y marks the Task done, ctrl-x then n leaves it" {
  fake_cli "$INBOX"
  attach
  tx send-keys -K -c "$C" C-b T
  sleep 1.5
  tx send-keys -K -c "$C" C-x
  sleep 0.5
  tx send-keys -K -c "$C" n
  sleep 1
  tx send-keys -K -c "$C" C-d
  sleep 0.5
  tx send-keys -K -c "$C" y
  wait_for 'grep -q "^task done t-000001 --json$" "$TMPDIR/bin/calls"'
  ! grep -q '^task drop' "$TMPDIR/bin/calls"
}

@test "prefix+T: Done that would lose work says why, and y removes it anyway" {
  fake_cli "$INBOX"
  echo "its branch is not merged" >"$TMPDIR/bin/lose"
  attach
  tx send-keys -K -c "$C" C-b T
  sleep 1.5
  tx send-keys -K -c "$C" C-d
  sleep 0.5
  tx send-keys -K -c "$C" y
  wait_for 'grep -q "^task done t-000001 --json$" "$TMPDIR/bin/calls"'
  sleep 0.5
  tx send-keys -K -c "$C" y
  wait_for 'grep -q "^task done t-000001 --force --json$" "$TMPDIR/bin/calls"'
}

@test "prefix+T: Enter on a Task with no session offers Drop" {
  fake_cli "$(printf '%s' "$INBOX" | jq -c '.tasks |= [.[3]]')"
  attach
  tx send-keys -K -c "$C" C-b T
  sleep 1.5
  tx send-keys -K -c "$C" Enter
  sleep 0.5
  tx send-keys -K -c "$C" x
  wait_for 'grep -q "^task drop t-000004 --json$" "$TMPDIR/bin/calls"'
}

@test "close.sh --task marks the session's Task once the session is gone" {
  new feature/closing --muxa-task t-0000e1 >/dev/null
  fake_cli '{}'
  "$ROOT/scripts/close.sh" --keep --task dropped feature-closing
  ! tx has-session -t =feature-closing 2>/dev/null
  [ "$(cat "$TMPDIR/bin/calls")" = 'task drop t-0000e1 --json' ]
}

@test "close.sh leaves the Task alone when the session cannot close" {
  new feature/dirty --muxa-task t-0000e2 >/dev/null
  touch "$(tx show-options -t =feature-dirty: -qv @agent_worktree)/wip"
  fake_cli '{}'
  run "$ROOT/scripts/close.sh" --task done feature-dirty
  [ "$status" -ne 0 ]
  tx has-session -t =feature-dirty
  [ ! -e "$TMPDIR/bin/calls" ]
}

@test "x in the tree on a Task's session asks, then closes it and marks the Task done" {
  new feature/menu --muxa-task t-0000e3 >/dev/null
  fake_cli '{}'
  attach
  sb="$(tx list-panes -t base: -F '#{pane_id} #{@agents_sidebar_pane}' | awk '$2 == 1 { print $1 }')"
  wait_for "tx capture-pane -p -t '$sb' | grep -q feature-menu"
  tx send-keys -K -c "$C" C-b Tab
  sleep 0.5
  for _ in 1 2 3; do
    tx capture-pane -e -p -t "$sb" | grep -q '48;5;236m.*feature-menu' && break
    tx send-keys -K -c "$C" k
    sleep 0.3
  done
  tx send-keys -K -c "$C" x
  sleep 0.5
  tx send-keys -K -c "$C" d
  wait_for '! tx has-session -t =feature-menu 2>/dev/null'
  # The sidebar also polls the Inbox, so look for the call among the others.
  wait_for 'grep -qx "task done t-0000e3 --json" "$TMPDIR/bin/calls" 2>/dev/null'
}

# A work:9 host (ADR 0048): prefix + N asks for the text first.
work9_tasks() {
  printf '{"workspaces":["work"],"workspaceInfo":[{"name":"work","root":"%s/repos","repos":["%s"]}],"capabilities":["task-records","work:9"],"truncated":false,"tasks":[]}' \
    "$TMPDIR" "$REPO"
}

@test "the form on work:9: s saves the text as a Capture and starts nothing" {
  fake_cli '{"workspaces":["work"],"capabilities":["task-records","work:9"],"truncated":false,"tasks":[]}'
  printf '{"capture":{"id":"c-0000abcd"}}\n' >"$TMPDIR/bin/out-capture"
  run bash -c "printf '%s\n%s' 'Onboarding could be shorter -v' s | '$ROOT/scripts/muxa.sh' form none ''"
  [ "$status" -eq 0 ]
  [[ "$output" == *"Saved in Unsorted: c-0000abcd"* ]]
  # The Inbox refreshes after it, so the call list, not the last args.
  grep -qx -- "capture - --source tmux --json" "$TMPDIR/bin/calls"
  [ "$(cat "$TMPDIR/bin/stdin")" = "Onboarding could be shorter -v" ]
  run grep -c "^task new" "$TMPDIR/bin/calls"
  [ "$output" = 0 ]
}

@test "the popup erases a whole Japanese character on Backspace" {
  fake_cli '{"workspaces":["work"],"capabilities":["task-records","work:9"],"truncated":false,"tasks":[]}'
  printf '{"capture":{"id":"c-0000abcd"}}\n' >"$TMPDIR/bin/out-capture"
  attach
  tx send-keys -K -c "$C" C-b N
  sleep 1.5
  tx send-keys -K -c "$C" -l "あい"
  tx send-keys -K -c "$C" BSpace
  answer "う"
  tx send-keys -K -c "$C" s
  wait_for '[ -e "$TMPDIR/bin/stdin" ]'
  [ "$(cat "$TMPDIR/bin/stdin")" = "あう" ]
}

@test "the form on work:9: a saves the text and asks the workspace's assistant" {
  fake_cli '{"workspaces":["work"],"capabilities":["task-records","work:9"],"truncated":false,"tasks":[]}'
  printf '{"capture":{"id":"c-0000abcd"},"request":{"id":"q-0000abcd","delivery":"pending"}}\n' >"$TMPDIR/bin/out-capture"
  run bash -c "printf '%s\n%s' 'Compare hosting plans -v' a | '$ROOT/scripts/muxa.sh' form none ''"
  [ "$status" -eq 0 ]
  [[ "$output" == *"Asked the assistant in work: q-0000abcd (pending)"* ]]
  grep -qx -- "capture - --source tmux --assist --workspace work --json" "$TMPDIR/bin/calls"
  [ "$(cat "$TMPDIR/bin/stdin")" = "Compare hosting plans -v" ]
  run grep -c "^task new" "$TMPDIR/bin/calls"
  [ "$output" = 0 ]
}

@test "the form on work:9: a with two workspaces asks which one" {
  fake_cli '{"workspaces":["home","work"],"capabilities":["task-records","work:9"],"truncated":false,"tasks":[]}'
  printf '{"capture":{"id":"c-0000abcd"},"request":{"id":"q-0000abcd","delivery":"pending"}}\n' >"$TMPDIR/bin/out-capture"
  run bash -c "printf '%s\n%s%s\n' 'Renew the visa' a work | '$ROOT/scripts/muxa.sh' form none ''"
  [ "$status" -eq 0 ]
  [[ "$output" == *"Asked the assistant in work: q-0000abcd (pending)"* ]]
  grep -qx -- "capture - --source tmux --assist --workspace work --json" "$TMPDIR/bin/calls"
}

@test "the form on work:9: q cancels and saves nothing" {
  fake_cli '{"workspaces":["work"],"capabilities":["task-records","work:9"],"truncated":false,"tasks":[]}'
  run bash -c "printf '%s\n%s' 'Not this' q | '$ROOT/scripts/muxa.sh' form none ''"
  [ "$status" -eq 0 ]
  [ "$(cat "$TMPDIR/bin/calls")" = "tasks --json" ]
}

@test "prefix+N with Work items: t takes the text into the New Task form" {
  new feature/from-text >/dev/null
  sid="$(tx display-message -p -t =feature-from-text: '#{session_id}')"
  fake_cli "$(printf '{"group":"no-agent","record":{"id":"t-0000d3"},"session":{"id":"%s","name":"feature-from-text"}}' "$sid")"
  work9_tasks >"$TMPDIR/bin/out-tasks"
  attach
  tx send-keys -K -c "$C" C-b N
  sleep 1.5
  answer "Shorten onboarding"
  tx send-keys -K -c "$C" t
  sleep 0.5
  answer "no repo"       # one workspace: the picker comes first
  answer ""              # title: the text's first line
  answer ""              # goal: the text
  wait_for '[ "$(tx display-message -p -c "$C" "#{session_name}")" = feature-from-text ]'
  args_are task new --workspace work --title "Shorten onboarding" --goal "Shorten onboarding" --json
  run grep -c "^capture" "$TMPDIR/bin/calls"
  [ "$output" = 0 ]
}

@test "prefix+N with Work items: no text opens the New Task form" {
  fake_cli "$(work9_tasks)"
  attach
  tx send-keys -K -c "$C" C-b N
  sleep 1.5
  answer ""              # no text
  answer "no repo"       # one workspace: the picker comes first
  answer ""              # title: empty refuses
  sleep 2.5
  run grep -cE "^(capture|task new)" "$TMPDIR/bin/calls"
  [ "$output" = 0 ]
}
