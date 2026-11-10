#!/usr/bin/env bats
# The Inbox at muxa's work level 12 (ADR 0051): `a` on a Work item asks which
# staff member takes it, a Work item shows its request's steps, and handoff
# items take Approve and Reject. The data is the contract's own fixture.
load helper

FIXTURE="$ROOT/../protocol/fixtures/assistant/messages.jsonl"

setup() {
  setup_server
  ITEMS="$TMPDIR/inbox.json"
  staff_inbox >"$ITEMS"
}
teardown() { kill "${CLIENT_PID:-}" 2>/dev/null || true; teardown_server; }

# The level 12 list: a handoff and a delegate item, a Work item whose request
# has two steps, and a workspace with a staff of four.
staff_inbox() {
  jq -c 'select(.type == "inbox-list" and (.assistant.delegates // [] | any(.staff))) |
    {work: 12, items, entries: [], runs: [], workItems: .work, captures, requests, plans, assistant}' \
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

@test "a handoff item names both members and its purpose, and takes approve, reject and later" {
  out="$(plain inbox view i-7a8b9c0d "$ITEMS")"
  [[ "$out" == *"From: lisk-strategist → lisk-architect · step 3"* ]]
  [[ "$out" == *"Ask: Check that option B fits the current gateway."* ]]
  [[ "$out" == *$'l Later\na Approve   r Reject'* ]]
  [ "$(inbox key i-7a8b9c0d a "$ITEMS")" = approve ]
  [ "$(inbox key i-7a8b9c0d r "$ITEMS")" = reject ]
  [ "$(inbox key i-7a8b9c0d l "$ITEMS")" = later ]
  [ "$(inbox key i-7a8b9c0d p "$ITEMS")" = protect ]
  [[ "$(plain inbox list "$ITEMS" | grep '^i-7a8b9c0d')" == *"👥 lisk-strategist asks lisk-architect"* ]]
}

@test "a delegate item names the staff member the plan picked" {
  [[ "$(plain inbox view i-8b9c0d1e "$ITEMS")" == *"To: lisk · lisk-security · research"* ]]
}

@test "a Work item shows its request's steps, and the next member a step hands to" {
  out="$(plain inbox view w-8b9c0d1e "$ITEMS")"
  [[ "$out" == *$'  1. lisk-chief-of-staff · resulted → lisk-strategist\n  2. lisk-strategist · resulted → lisk-architect'* ]]
}

@test "the staff picker numbers the members, the lead marked, and Enter leaves the lead" {
  run bash -c "printf '\n' | '$ROOT/scripts/inbox.sh' worker w-8b9c0d1e '$ITEMS' 2>'$TMPDIR/err'"
  [ "$status" -eq 0 ]
  [ -z "$output" ]
  err="$(strip_ansi <"$TMPDIR/err")"
  [[ "$err" == *"1  lisk-chief-of-staff  lead · Chief of staff"* ]]
  [[ "$err" == *"4  lisk-security  Security"* ]]
  [ "$(printf '2\n' | inbox worker w-8b9c0d1e "$ITEMS" 2>/dev/null)" = lisk-strategist ]
  [ "$(printf 'lisk-architect\n' | inbox worker w-8b9c0d1e "$ITEMS" 2>/dev/null)" = lisk-architect ]
  run bash -c "printf '9\n' | '$ROOT/scripts/inbox.sh' worker w-8b9c0d1e '$ITEMS' 2>/dev/null"
  [ "$status" -ne 0 ]
  [ -z "$output" ]
}

@test "below work:12, or with one member, nothing is asked" {
  jq -c '.work = 11' "$ITEMS" >"$TMPDIR/11.json"
  [ -z "$(printf '2\n' | inbox worker w-8b9c0d1e "$TMPDIR/11.json" 2>&1)" ]
  jq -c '.assistant.delegates[0].staff |= .[:1]' "$ITEMS" >"$TMPDIR/one.json"
  [ -z "$(printf '2\n' | inbox worker w-8b9c0d1e "$TMPDIR/one.json" 2>&1)" ]
}

@test "prefix+T: a on a Work item asks for the member and passes it" {
  fake_cli
  # The Work item is the first row: no Today, no items.
  jq -c '.items = [] | .plans = [] | .assistant.overall = null' "$ITEMS" >"$ITEMS.tmp" && mv "$ITEMS.tmp" "$ITEMS"
  attach
  key C-b T
  sleep 1
  key a
  sleep 0.5
  key 2 Enter
  wait_for 'grep -q "^assist w-8b9c0d1e --worker lisk-strategist --json$" "$TMPDIR/bin/calls"'
}
