#!/usr/bin/env bats
# The Inbox at muxa's work level 13 (ADR 0052): a write item shows its target
# and its content whole, approve asks once more with the target, and reject
# takes a reason. The data is the contract's own fixture.
load helper

FIXTURE="$ROOT/../protocol/fixtures/assistant/messages.jsonl"

setup() {
  setup_server
  ITEMS="$TMPDIR/inbox.json"
  writes_inbox >"$ITEMS"
}
teardown() { kill "${CLIENT_PID:-}" 2>/dev/null || true; teardown_server; }

# The level 13 list: two open write items, one from an older result, and a
# workspace with a writer.
writes_inbox() {
  jq -c 'select(.type == "inbox-list" and (.assistant.delegates // [] | any(.writer))) |
    {work: 13, items, entries: [], runs: [], workItems: .work, captures, requests, plans, assistant}' \
    "$FIXTURE"
}

inbox() { "$ROOT/scripts/inbox.sh" "$@"; }
plain() { "$@" | strip_ansi; }

# A muxa-cli that prints $ITEMS for every call and logs one line per call.
fake_cli() {
  mkdir -p "$TMPDIR/bin"
  cat >"$TMPDIR/bin/muxa-cli" <<FAKE
#!/bin/sh
(shift 2; echo "\$*") >>"$TMPDIR/bin/calls"
cat "$ITEMS"
FAKE
  chmod +x "$TMPDIR/bin/muxa-cli"
  tx set-option -g @agents_muxa_cli "$TMPDIR/bin/muxa-cli"
  tx set-option -g @agents_muxa auto
}

attach() {
  load_plugin
  python3 "$BATS_TEST_DIRNAME/pty-client.py" "$TMUX_AGENTS_SOCKET" base 160 40 30 3>&- &
  CLIENT_PID=$!
  wait_for '[ -n "$(tx list-clients -F "#{client_name}")" ]'
  C="$(tx list-clients -F '#{client_name}')"
}

key() { tx send-keys -K -c "$C" "$@"; sleep 0.5; }

@test "a write item shows its target and its content whole, and takes approve, reject and later" {
  out="$(plain inbox view i-0d1e2f3a "$ITEMS")"
  [[ "$out" == *"To: linear:ENG-12"* ]]
  [[ "$out" == *"From: lisk-delivery · v2"* ]]
  [[ "$out" == *$'Content, written exactly as below:\nThe fix ships in 2.4.'* ]]
  [[ "$out" == *$'l Later\na Approve   r Reject'* ]]
  [ "$(inbox key i-0d1e2f3a a "$ITEMS")" = approve ]
  [ "$(inbox key i-0d1e2f3a r "$ITEMS")" = reject ]
  [[ "$(plain inbox list "$ITEMS" | grep '^i-0d1e2f3a')" == *"📤 lisk-delivery proposes to write to linear:ENG-12"* ]]
}

@test "every line of the content is shown, and an older result's write says so" {
  jq -c '.items[0].write.content = "one\n\ntwo\n\tthree\u001b[31m"' "$ITEMS" >"$TMPDIR/long.json"
  out="$(inbox view i-0d1e2f3a "$TMPDIR/long.json")"
  [[ "$out" == *$'below:\none\n\ntwo\n\tthree'* ]]
  [[ "$out" != *$'\e[31m'* ]]
  [[ "$(plain inbox view i-1e2f3a4b "$ITEMS")" == *"From: lisk-delivery · v1 · from an older result"* ]]
  [[ "$(plain inbox view i-0d1e2f3a "$ITEMS")" != *"older result"* ]]
}

@test "prefix+T: approve asks once more with the target, and n sends nothing" {
  fake_cli
  jq -c '.items |= .[:1] | .workItems = [] | .plans = [] | .assistant.overall = null' "$ITEMS" >"$ITEMS.tmp" && mv "$ITEMS.tmp" "$ITEMS"
  attach
  key C-b T
  sleep 1
  key a
  sleep 0.5
  key n
  sleep 1
  run grep -q "answer i-0d1e2f3a approve" "$TMPDIR/bin/calls"
  [ "$status" -ne 0 ]
  key a
  sleep 0.5
  key y
  wait_for 'grep -q "^inbox answer i-0d1e2f3a approve --json$" "$TMPDIR/bin/calls"'
}

@test "prefix+T: reject on a write item asks for an optional reason" {
  fake_cli
  jq -c '.items |= .[:1] | .workItems = [] | .plans = [] | .assistant.overall = null' "$ITEMS" >"$ITEMS.tmp" && mv "$ITEMS.tmp" "$ITEMS"
  attach
  key C-b T
  sleep 1
  key r
  key Wrong Enter
  wait_for 'grep -q "^inbox answer i-0d1e2f3a reject --reason Wrong --json$" "$TMPDIR/bin/calls"'
}
