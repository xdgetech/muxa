#!/usr/bin/env bats
# A workspace assistant's new result: its Work item shows `· new result` and
# sorts first until the user looks at it, and the Inbox poll notifies once.
# The data is the contract's own work:9 list.
load helper

setup() {
  setup_server
  ITEMS="$TMPDIR/inbox.json"
  STATE="$XDG_STATE_HOME/muxa/tmux-results.json"
  jq -c 'select(.type == "inbox-list" and .plans == null and
      ((.assistant.delegates // []) | any(.staff) | not)) |
    {work: 9, items: [], entries: [], runs: [], workItems: .work, captures, requests}' \
    "$ROOT/../protocol/fixtures/assistant/messages.jsonl" >"$ITEMS"
}
teardown() { kill "${CLIENT_PID:-}" 2>/dev/null || true; teardown_server; }

inbox() { "$ROOT/scripts/inbox.sh" "$@"; }
results() { "$ROOT/scripts/results.sh" "$@"; }
work_rows() { inbox list "$1" | grep '^w-' | cut -f2 | strip_ansi; }

# The visa request (q-1b2c3d4e, Work item w-5f6a7b8c) gets its result.
visa_resulted() {
  jq -c '(.requests[] | select(.id == "q-1b2c3d4e")) |= (.delivery = "resulted" | .deliveryAt = 1762660500)' \
    "$ITEMS" >"$ITEMS.tmp" && mv "$ITEMS.tmp" "$ITEMS"
}

fake_cli() {
  mkdir -p "$TMPDIR/bin"
  printf '#!/bin/sh\ncat "%s"\n' "$ITEMS" >"$TMPDIR/bin/muxa-cli"
  chmod +x "$TMPDIR/bin/muxa-cli"
  tx set-option -g @agents_muxa_cli "$TMPDIR/bin/muxa-cli"
  tx set-option -g @agents_muxa auto
}

recorder() {
  printf '#!/bin/sh\necho "$1|$2" >>"%s/notified"\n' "$TMPDIR" >"$TMPDIR/rec"
  chmod +x "$TMPDIR/rec"
  tx set-option -g @agents_notify on
  tx set-option -g @agents_notify_cmd "$TMPDIR/rec"
}
notified() { cat "$TMPDIR/notified" 2>/dev/null; }

@test "the first poll keeps the results already there as seen" {
  results track "$ITEMS"
  [ "$(results new "$ITEMS")" = "[]" ]
  [[ "$(work_rows "$ITEMS")" != *"new result"* ]]
}

@test "a new result shows · new result and sorts first; viewing it clears it" {
  results track "$ITEMS"
  visa_resulted
  results track "$ITEMS"
  run work_rows "$ITEMS"
  [[ "${lines[0]}" == "◆ Check what the visa renewal needs · new result  in progress · asked: resulted · "* ]]
  [[ "${lines[1]}" == "◆ Draft the merged onboarding page  "* ]]
  [ "$(inbox list "$ITEMS" | grep '^sec:work' | cut -f2 | strip_ansi)" = "Work 4 · 1 new" ]
  inbox view w-5f6a7b8c "$ITEMS" >/dev/null
  [ "$(inbox list "$ITEMS" | grep '^sec:work' | cut -f2 | strip_ansi)" = "Work 4" ]
  run work_rows "$ITEMS"
  [[ "${lines[0]}" == "◆ Draft the merged onboarding page  "* ]]
  [[ "$output" != *"new result"* ]]
}

@test "a new version of a seen result is new again" {
  results track "$ITEMS"
  jq -c '(.requests[] | select(.id == "q-0a1b2c3d")) |= (.version = 3 | .deliveryAt = 1762670000)' \
    "$ITEMS" >"$ITEMS.tmp" && mv "$ITEMS.tmp" "$ITEMS"
  results track "$ITEMS"
  [ "$(results new "$ITEMS")" = '["w-6a7b8c9d"]' ]
}

@test "Work items no longer listed leave the state" {
  results track "$ITEMS"
  jq -c '.workItems |= map(select(.id != "w-6a7b8c9d"))' "$ITEMS" >"$ITEMS.tmp" && mv "$ITEMS.tmp" "$ITEMS"
  results track "$ITEMS"
  [ "$(jq -c 'keys' "$STATE")" = '[]' ]
}

@test "the poll notifies once per result, also on a new server" {
  fake_cli
  recorder
  inbox cache --force
  visa_resulted
  jq -c '(.workItems[] | select(.id == "w-5f6a7b8c")).title |= "Check \u001b[31mvisa\u001b[0m"' \
    "$ITEMS" >"$ITEMS.tmp" && mv "$ITEMS.tmp" "$ITEMS"
  inbox cache --force
  inbox cache --force
  [ "$(notified)" = "Inbox|Assistant result: Check [31mvisa[0m" ]
  tx kill-server
  tx -f /dev/null new-session -d -s base -c "$TMPDIR"
  fake_cli
  recorder
  inbox cache --force
  [ "$(notified | wc -l | tr -d ' ')" = 1 ]
  [[ "$(work_rows "$ITEMS" | head -n 1)" == *"· new result"* ]]
}

@test "a result notifies only when result is in @agents_notify_events" {
  fake_cli
  recorder
  tx set-option -g @agents_notify_events 'waiting done error'
  inbox cache --force
  visa_resulted
  inbox cache --force
  [ -z "$(notified)" ]
  [ "$(results new "$ITEMS")" = '["w-5f6a7b8c"]' ]
}

@test "prefix+T: the Work item under the cursor is looked at" {
  fake_cli
  inbox cache --force
  visa_resulted
  inbox cache --force
  [ "$(results new "$ITEMS")" = '["w-5f6a7b8c"]' ]
  load_plugin
  python3 "$BATS_TEST_DIRNAME/pty-client.py" "$TMUX_AGENTS_SOCKET" base 160 40 30 3>&- &
  CLIENT_PID=$!
  wait_for '[ -n "$(tx list-clients -F "#{client_name}")" ]'
  tx send-keys -K -c "$(tx list-clients -F '#{client_name}')" C-b T
  wait_for '[ "$(results new "$ITEMS")" = "[]" ]'
}
