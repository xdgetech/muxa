#!/usr/bin/env bats
# The Inbox at muxa's work level 11 (ADR 0050): Today in the order the user
# chose, K/J/x on its rows, hypothesis items, and the Policies list (y). The
# data is the contract's own fixture.
load helper

FIXTURE="$ROOT/../protocol/fixtures/assistant/messages.jsonl"

setup() {
  setup_server
  ITEMS="$TMPDIR/inbox.json"
  memory_inbox >"$ITEMS"
}
teardown() { kill "${CLIENT_PID:-}" 2>/dev/null || true; teardown_server; }

# The level 10 list (Work items, sort and delegate items) with the level 11
# list's plan, hypothesis item, policies, hypotheses and corrections.
memory_inbox() {
  jq -c -n --slurpfile m "$FIXTURE" '
    ($m[] | select(.type == "inbox-list" and .plans and .policies == null)) as $nine |
    ($m[] | select(.type == "inbox-list" and .policies)) as $ten |
    {work: 11, items: ($nine.items + $ten.items), entries: [], runs: [], workItems: $nine.work,
     captures: $nine.captures, requests: $nine.requests, plans: $ten.plans, assistant: $nine.assistant,
     policies: $ten.policies, hypotheses: $ten.hypotheses, corrections: $ten.corrections}'
}

inbox() { "$ROOT/scripts/inbox.sh" "$@"; }
policies() { "$ROOT/scripts/policies.sh" "$@"; }
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

@test "Today follows the order the user chose, and a dismissed row is one line" {
  out="$(plain inbox list "$ITEMS")"
  [[ "$out" == *$'today:1\t  1. Compare vendor offers  30–60 min'* ]]
  [[ "$out" == *$'today:2\t  2. Prepare the quarterly report  60–120 min'* ]]
  [[ "$out" == *$'today:3\t  3. Try the tool for business B · not today'* ]]
  [ "$(inbox target today:1 "$ITEMS")" = w-7b8c9d0e ]
  [ "$(inbox target today:3 "$ITEMS")" = i-1a2b3c4d ]
}

@test "a Today row cites the records by their text, and a gone record as forgotten" {
  out="$(plain inbox view today:2 "$ITEMS")"
  [[ "$out" == *"policy: This week, recruiting comes before sales and support.  y-0a1b2c3d"* ]]
  [[ "$out" == *"correction: pinned w-0b1c2d3e  x-0a1b2c3d"* ]]
  [[ "$out" == *"K/J Move   x Not today   o Open   t Plan Now"* ]]
  jq -c '.hypotheses = []' "$ITEMS" >"$TMPDIR/gone.json"
  [[ "$(plain inbox view today:1 "$TMPDIR/gone.json")" == *"hypothesis: forgotten  h-4e5f6a7b"* ]]
  out="$(plain inbox view today:3 "$ITEMS")"
  [[ "$out" == *"i-1a2b3c4d · not today"* ]]
  [[ "$out" == *"K/J Move   o Open   t Plan Now"* ]]
  [[ "$(plain inbox view today: "$ITEMS")" == *"t Plan Now   y Policies"* ]]
}

@test "K and J move a Today row, x takes it off Today, y opens the Policies list" {
  [ "$(inbox key today:1 K "$ITEMS")" = "" ]
  [ "$(inbox key today:1 J "$ITEMS")" = plan:down ]
  [ "$(inbox key today:2 K "$ITEMS")" = plan:up ]
  [ "$(inbox key today:3 J "$ITEMS")" = "" ]
  [ "$(inbox key today:2 x "$ITEMS")" = plan:dismiss ]
  [ "$(inbox key today:3 x "$ITEMS")" = "" ]
  [ "$(inbox key today: x "$ITEMS")" = "" ]
  [ "$(inbox key i-1a2b3c4d y "$ITEMS")" = policies ]
  [ "$(inbox order today:1 down "$ITEMS" | tr '\n' ' ')" = "w-0b1c2d3e w-7b8c9d0e i-1a2b3c4d " ]
  [ "$(inbox order today:3 up "$ITEMS" | tr '\n' ' ')" = "w-7b8c9d0e i-1a2b3c4d w-0b1c2d3e " ]
  [ "$(inbox order today:3 down "$ITEMS")" = "" ]
}

@test "below work:11 the Today keys and y do nothing" {
  jq -c '.work = 10' "$ITEMS" >"$TMPDIR/nine.json"
  for k in K J x y; do
    [ "$(inbox key today:2 "$k" "$TMPDIR/nine.json")" = "" ]
  done
  [[ "$(plain inbox view today:2 "$TMPDIR/nine.json")" != *"K/J Move"* ]]
}

@test "a hypothesis item shows its scope and evidence, and takes confirm, reject and later" {
  out="$(plain inbox view i-5e6f7a8b "$ITEMS")"
  [[ "$out" == *"Scope: all · confidence low"* ]]
  [[ "$out" == *"Based on 2 corrections: x-0a1b2c3d x-2e3f4a5b"* ]]
  [[ "$out" == *$'l Later\na Confirm   r Reject'* ]]
  [ "$(inbox key i-5e6f7a8b a "$ITEMS")" = confirm ]
  [ "$(inbox key i-5e6f7a8b r "$ITEMS")" = reject ]
  [ "$(inbox key i-5e6f7a8b l "$ITEMS")" = later ]
}

@test "the Policies list holds the policies, then hypotheses, then corrections" {
  run bash -c "'$ROOT/scripts/policies.sh' list '$ITEMS' | cut -f1"
  [ "$output" = "$(printf '%s\n' add: P: y-0a1b2c3d y-2c3d4e5f y-3d4e5f6a H: h-4e5f6a7b h-1a2b3c4d \
    X: x-3f4a5b6c x-2e3f4a5b x-1d2e3f4a x-0b1c2d3e x-0a1b2c3d)" ]
  out="$(plain policies list "$ITEMS")"
  [[ "$out" == *"Finish the tax papers before anything personal.  personal · from 2025-10-30 04:53Z · until 2025-11-04 23:46Z · expired"* ]]
  [[ "$out" == *"Mornings in work are for the report.  work · no end"* ]]
  [[ "$out" == *"Work due this week comes first.  proposed · low · all"* ]]
  [[ "$out" == *"not today i-1a2b3c4d  2025-11-10 11:43Z · Waiting for the vendor."* ]]
}

@test "each record shows its keys: edit and delete a policy, confirm, reject or forget the rest" {
  [[ "$(plain policies view y-2c3d4e5f "$ITEMS")" == *$'confirmed from h-3c4d5e6f\n\ne Edit   d Delete'* ]]
  [[ "$(plain policies view h-4e5f6a7b "$ITEMS")" == *"a Confirm   r Reject   f Forget"* ]]
  # A rejected hypothesis has no open item: only Forget.
  out="$(plain policies view h-1a2b3c4d "$ITEMS")"
  [[ "$out" == *"based on 1 correction: x-0a1b2c3d"* ]]
  [[ "$out" == *$'\nf Forget'* && "$out" != *"Confirm"* ]]
  [[ "$(plain policies view x-2e3f4a5b "$ITEMS")" == *"Order: w-7b8c9d0e w-0b1c2d3e i-1a2b3c4d"* ]]
  [ "$(policies key y-0a1b2c3d e "$ITEMS")" = edit ]
  [ "$(policies key y-0a1b2c3d d "$ITEMS")" = delete ]
  [ "$(policies key y-0a1b2c3d f "$ITEMS")" = "" ]
  [ "$(policies key h-4e5f6a7b a "$ITEMS")" = confirm:i-5e6f7a8b ]
  [ "$(policies key h-4e5f6a7b r "$ITEMS")" = reject:i-5e6f7a8b ]
  [ "$(policies key h-1a2b3c4d a "$ITEMS")" = "" ]
  [ "$(policies key h-1a2b3c4d f "$ITEMS")" = forget ]
  [ "$(policies key x-0a1b2c3d f "$ITEMS")" = forget ]
  [ "$(policies key x-0a1b2c3d e "$ITEMS")" = "" ]
  [ "$(policies key P: n "$ITEMS")" = add ]
}

@test "prefix+T: J moves a Today row, x takes one off Today with a reason" {
  fake_cli
  attach
  key C-b T
  sleep 1
  key j J
  wait_for 'grep -q "^plan reorder w-0b1c2d3e w-7b8c9d0e i-1a2b3c4d --json$" "$TMPDIR/bin/calls"'
  sleep 1
  # The cursor followed the row down to today:2.
  key x
  key Waiting Enter
  wait_for 'grep -q "^plan dismiss w-0b1c2d3e --reason Waiting --json$" "$TMPDIR/bin/calls"'
}

@test "prefix+T: y opens the Policies list, n adds a policy that ends this week" {
  fake_cli
  attach
  key C-b T
  sleep 1
  key y
  sleep 1
  key n
  key Recruiting Enter
  key Enter
  key w Enter
  wait_for 'grep -q "^policy add Recruiting --until week --json$" "$TMPDIR/bin/calls"'
}
