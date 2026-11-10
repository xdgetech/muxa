#!/usr/bin/env bats
# The Inbox at muxa's work level 14 (ADR 0053): a memory item shows a Task's
# text for the workspace memory whole, and approve asks once more with the
# workspace. The data is the contract's own fixture.
load helper

FIXTURE="$ROOT/../protocol/fixtures/items/messages.jsonl"

setup() {
  setup_server
  ITEMS="$TMPDIR/inbox.json"
  memory_inbox >"$ITEMS"
}
teardown() { kill "${CLIENT_PID:-}" 2>/dev/null || true; teardown_server; }

# The level 14 list: open memory items.
memory_inbox() {
  jq -c 'select(.type == "inbox-list" and any(.items[]; .kind == "memory")) |
    {work: 14, items, entries: [], runs: [], workItems: [], captures: [], requests: [], plans: []}' \
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

@test "a memory item shows its Task and its text whole, and takes approve, reject and later" {
  out="$(plain inbox view i-4e5f6a7b "$ITEMS")"
  [[ "$out" == *"From: Task t-00b001"* ]]
  [[ "$out" == *$'exactly as below:\n- API keys live in Vault under kv/work/api'* ]]
  [[ "$out" == *"- Decision: keys rotate every 90 days, because the vendor expires them at 100."* ]]
  [[ "$out" == *$'l Later\na Approve   r Reject'* ]]
  [ "$(inbox key i-4e5f6a7b a "$ITEMS")" = approve ]
  [ "$(inbox key i-4e5f6a7b r "$ITEMS")" = reject ]
  [ "$(inbox key i-4e5f6a7b l "$ITEMS")" = later ]
  [[ "$(plain inbox list "$ITEMS" | grep '^i-4e5f6a7b')" == *"🧠 Rotate the API keys proposes to add to the workspace memory"* ]]
}

@test "every line of the text is shown, without terminal control characters" {
  jq -c '.items[0].memory.text = "one\n\ntwo\n\tthree\u001b[31m"' "$ITEMS" >"$TMPDIR/long.json"
  out="$(inbox view i-4e5f6a7b "$TMPDIR/long.json")"
  [[ "$out" == *$'below:\none\n\ntwo\n\tthree'* ]]
  [[ "$out" != *$'\e[31m'* ]]
}

@test "prefix+T: approve asks once more with the workspace, sends the shown hash, and n sends nothing" {
  fake_cli
  jq -c '.items |= .[:1]' "$ITEMS" >"$ITEMS.tmp" && mv "$ITEMS.tmp" "$ITEMS"
  attach
  key C-b T
  sleep 1
  key a
  sleep 0.5
  key n
  sleep 1
  run grep -q "answer i-4e5f6a7b approve" "$TMPDIR/bin/calls"
  [ "$status" -ne 0 ]
  key a
  sleep 0.5
  key y
  hash="$(jq -r '.items[0].memory.hash' "$ITEMS")"
  wait_for 'grep -qF "inbox answer i-4e5f6a7b approve --hash $hash --json" "$TMPDIR/bin/calls"'
}

@test "prefix+T: reject on a memory item sends no reason" {
  fake_cli
  jq -c '.items |= .[:1]' "$ITEMS" >"$ITEMS.tmp" && mv "$ITEMS.tmp" "$ITEMS"
  attach
  key C-b T
  sleep 1
  key r
  wait_for 'grep -q "^inbox answer i-4e5f6a7b reject --json$" "$TMPDIR/bin/calls"'
}
