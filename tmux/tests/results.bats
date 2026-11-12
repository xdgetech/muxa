#!/usr/bin/env bats
# Assistant results in tmux. At work:16 every Result is a result item
# (ADR 0058): a Hand off, s Send, d Done, and s Revise on a write item. The
# Inbox poll still notifies once per result; Work rows carry no "new result"
# mark, since the item's unread dot is that mark. The data is the contract's.
load helper

FIXTURE="$ROOT/../protocol/fixtures/assistant/messages.jsonl"

setup() {
  setup_server
  ITEMS="$TMPDIR/inbox.json"
  jq -c 'select(.type == "inbox-list" and .plans == null and
      ((.assistant.delegates // []) | any(.staff) | not) and all(.items[]; .kind != "result")) |
    {work: 9, items: [], entries: [], runs: [], workItems: .work, captures, requests}' \
    "$FIXTURE" >"$ITEMS"
  REVIEW="$TMPDIR/review.json"
  jq -c 'select(.type == "inbox-list" and any(.items[]; .kind == "result")) |
    {work: 16, items, entries: [], runs: []}' "$FIXTURE" >"$REVIEW"
  STATE="$XDG_STATE_HOME/muxa/tmux-results.json"
}
teardown() { kill "${CLIENT_PID:-}" 2>/dev/null || true; teardown_server; }

inbox() { "$ROOT/scripts/inbox.sh" "$@"; }
results() { "$ROOT/scripts/results.sh" "$@"; }
plain() { "$@" | strip_ansi; }
work_rows() { inbox list "$1" | grep '^w-' | cut -f2 | strip_ansi; }

# The visa request (q-1b2c3d4e, Work item w-5f6a7b8c) gets its result.
visa_resulted() {
  jq -c '(.requests[] | select(.id == "q-1b2c3d4e")) |= (.delivery = "resulted" | .deliveryAt = 1762660500)' \
    "$ITEMS" >"$ITEMS.tmp" && mv "$ITEMS.tmp" "$ITEMS"
}

# A muxa-cli that prints <file> for every call and logs one line per call.
fake_cli() {
  mkdir -p "$TMPDIR/bin"
  cat >"$TMPDIR/bin/muxa-cli" <<FAKE
#!/bin/sh
(shift 2; echo "\$*") >>"$TMPDIR/bin/calls"
cat "$1"
FAKE
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

attach() {
  load_plugin
  python3 "$BATS_TEST_DIRNAME/pty-client.py" "$TMUX_AGENTS_SOCKET" base 160 40 30 3>&- &
  CLIENT_PID=$!
  wait_for '[ -n "$(tx list-clients -F "#{client_name}")" ]'
  C="$(tx list-clients -F '#{client_name}')"
}

key() { tx send-keys -K -c "$C" "$@"; sleep 0.5; }

@test "a result item shows the Result's lists and its hand-off, with a, s, d and l" {
  out="$(plain inbox view i-9c0d1e2f "$REVIEW")"
  [[ "$out" == *"From: lisk-strategist · v2 step 2"* ]]
  [[ "$out" == *$'Findings\n  - Option B needs one hire fewer.'* ]]
  [[ "$out" == *$'Open questions\n  - Is the Q1 budget fixed?'* ]]
  [[ "$out" == *$'Next steps\n  - Ask finance for the Q1 budget.'* ]]
  [[ "$out" == *$'Files\n  - /home/me/lisk/plans/roadmap-options.md'* ]]
  [[ "$out" == *"Hand off to lisk-architect: Check that option B fits the current gateway."* ]]
  [[ "$out" == *$'l Later   d ✓ Done'* ]]
  [[ "$out" == *"a Hand off to lisk-architect   s Send"* ]]
  [ "$(inbox key i-9c0d1e2f a "$REVIEW")" = approve ]
  [ "$(inbox key i-9c0d1e2f s "$REVIEW")" = reply ]
  [ "$(inbox key i-9c0d1e2f d "$REVIEW")" = dismiss ]
  [ "$(inbox key i-9c0d1e2f l "$REVIEW")" = later ]
  [[ "$(plain inbox list "$REVIEW" | grep '^i-9c0d1e2f')" == *"📋 lisk-strategist asks lisk-architect"* ]]
}

@test "a result without a staff has no hand-off: s Send and d Done" {
  out="$(plain inbox view i-a0b1c2d3 "$REVIEW")"
  [[ "$out" == *"From: the assistant · v1 step 1"* ]]
  [[ "$out" == *"s sends the assistant what to do next; d closes it."* ]]
  [ -z "$(inbox key i-a0b1c2d3 a "$REVIEW")" ]
  [ "$(inbox key i-a0b1c2d3 s "$REVIEW")" = reply ]
}

@test "a write item at work:16 takes s Revise" {
  out="$(plain inbox view i-0d1e2f3a "$REVIEW")"
  [[ "$out" == *"s asks lisk-delivery for a new draft."* ]]
  [[ "$out" == *"a Approve   r Reject   s Revise"* ]]
  [ "$(inbox key i-0d1e2f3a s "$REVIEW")" = reply ]
}

@test "prefix+T: s on a result asks what to do next and sends it" {
  fake_cli "$REVIEW"
  attach
  key C-b T
  sleep 1
  key s
  sleep 0.5
  key Budget Enter
  wait_for 'grep -qx "inbox answer i-9c0d1e2f reply --text Budget --json" "$TMPDIR/bin/calls"'
}

@test "Work rows carry no new-result mark; a new result is a result item" {
  visa_resulted
  results track "$ITEMS"
  [[ "$(work_rows "$ITEMS")" != *"new result"* ]]
  [ "$(inbox list "$ITEMS" | grep '^sec:work' | cut -f2 | strip_ansi)" = "Work 4" ]
}

@test "Work items no longer listed leave the state" {
  results track "$ITEMS"
  jq -c '.workItems |= map(select(.id != "w-6a7b8c9d"))' "$ITEMS" >"$ITEMS.tmp" && mv "$ITEMS.tmp" "$ITEMS"
  results track "$ITEMS"
  [ "$(jq -c 'keys' "$STATE")" = '[]' ]
}

@test "the poll notifies once per result, also on a new server" {
  fake_cli "$ITEMS"
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
  fake_cli "$ITEMS"
  recorder
  inbox cache --force
  [ "$(notified | wc -l | tr -d ' ')" = 1 ]
}

@test "a new version of a notified result notifies again" {
  fake_cli "$ITEMS"
  recorder
  inbox cache --force
  jq -c '(.requests[] | select(.id == "q-0a1b2c3d")) |= (.version = 3 | .deliveryAt = 1762670000)' \
    "$ITEMS" >"$ITEMS.tmp" && mv "$ITEMS.tmp" "$ITEMS"
  inbox cache --force
  [ "$(notified | wc -l | tr -d ' ')" = 1 ]
}

@test "a result notifies only when result is in @agents_notify_events" {
  fake_cli "$ITEMS"
  recorder
  tx set-option -g @agents_notify_events 'waiting done error'
  inbox cache --force
  visa_resulted
  inbox cache --force
  [ -z "$(notified)" ]
}
