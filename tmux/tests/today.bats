#!/usr/bin/env bats
# Today rows that say what to do (ADR 0059): each row names its action, the
# preview leads with Do, Enter, s (who gets the text) and the plan keys, and
# t is Re-rank. The items and the plan are the contract's fixtures.
load helper

setup() {
  setup_server
  ITEMS="$TMPDIR/today.json"
  today_inbox 17 >"$ITEMS"
}
teardown() { kill "${CLIENT_PID:-}" 2>/dev/null || true; teardown_server; }

inbox() { "$ROOT/scripts/inbox.sh" "$@"; }
plain_view() { inbox view "$1" "$2" | strip_ansi; }

# A work:<level> Inbox whose plan has one row per kind of target: a Task's
# review (with a session), a Task's menu, a result, a write, a briefing that
# asks, a failure, a memory proposal and a Work item.
today_inbox() {
  jq -cs --argjson level "$1" '
    ([.[] | select(.type == "inbox-list") | .items[]]) as $all |
    def pick($id; $kind): [$all[] | select(.id == $id and .kind == $kind)] | max_by(.answers | length);
    [.[] | select(.type == "inbox-list" and .plans and .policies == null)][0] as $l |
    [pick("i-2c3d4e5f"; "review"), pick("i-0a1b2c3d"; "ask"), pick("i-9c0d1e2f"; "result"),
     pick("i-0d1e2f3a"; "write"),
     (pick("e-1a2b3c4d"; "proposal") | .id = "e-2b3c4d5e" | .kind = "briefing" |
       .question = "Lisk v0.7 is due tomorrow. Ship it?" | .answers = ["dismiss", "ask-agent", "later"] |
       del(.proposal)),
     pick("i-3d4e5f6a"; "failure"), pick("i-6a7b8c9d"; "memory")] as $items |
    {work: $level, entries: [], runs: [], items: $items, workItems: $l.work,
     captures: $l.captures, requests: $l.requests,
     assistant: ($l.assistant + {overallRuns: {left: 2, max: 3}}),
     plans: [$l.plans[0] | del(.userOrder, .dismissed, .changed, .dueSoon) |
       .top = ([($items[] | {item: .id}), {work: "w-7b8c9d0e"}] |
         map(. + {why: "It waits.", next: "Do it now.", minutes: {low: 5, high: 10, estimated: 7}}))]}
  ' "$ROOT/../protocol/fixtures/items/messages.jsonl" "$ROOT/../protocol/fixtures/assistant/messages.jsonl"
}

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

@test "each Today row names its action from the kind of its target" {
  out="$(inbox list "$ITEMS" | grep '^today:[0-9]' | cut -f2 | strip_ansi)"
  [ "$out" = "$(printf '%s\n' \
    '▶ 1. Review changes · Reduce GA cost  5–10 min' \
    '  2. Allow or deny · Rotate the API keys  5–10 min' \
    '  3. Read result · Decide the Q1 roadmap  5–10 min' \
    '  4. Check text to send · Comment on ENG-12  5–10 min' \
    '  5. Decide · Close the stale issue ENG-12  5–10 min' \
    '  6. Fix · Nightly import  5–10 min' \
    '  7. Check memory note · Draft the onboarding guide  5–10 min' \
    '  8. Start · Compare vendor offers  5–10 min')" ]
  # A briefing that asks nothing is to read; a gone target is to open.
  jq -c '.items[4].question = "Lisk v0.7 shipped." | .items[6].id = "i-0000dead"' "$ITEMS" >"$TMPDIR/b.json"
  out="$(inbox list "$TMPDIR/b.json" | strip_ansi)"
  [[ "$out" == *$'today:5\t  5. Read · '* ]]
  [[ "$out" == *$'today:7\t  7. Open · '* ]]
}

@test "a Today row leads with Do, Enter, s, the other keys and the plan keys" {
  out="$(plain_view today:1 "$ITEMS")"
  [[ "$out" == *"1. Review changes · Reduce GA cost"* ]]
  [[ "$out" == *$'Do: Do it now.\nEnter  Open the Task (review)\ns      Send to the Task\'s agent…\nalso:  c Complete Task   d ✓ Done   l Later   o Go to the item\nplan:  K/J move   x not today   t Re-rank (2 of 3 runs left today)\n\nWhy: It waits.\nTime: 5–10 min, about 7'* ]]
  [[ "$out" != *"Next:"* ]]
  out="$(plain_view today:2 "$ITEMS")"
  [[ "$out" == *$'Enter  Open the Task (allow or deny)\ns      Deny & send to the Task\'s agent…\nalso:  a Allow   r Deny   l Later   o Go to the item'* ]]
  [[ "$(plain_view today:8 "$ITEMS")" == *$'Enter  Open the Work item\ns      Send to the secretary (secretary), with this Work item…\nalso:  d Complete   l Later   a Ask Assistant   P Pin'* ]]
  [[ "$(plain_view today: "$ITEMS")" == *"t Re-rank: re-rank the queue from what is open now (2 of 3 runs left today)"* ]]
}

@test "s on a Today row names who gets the text, by the kind of its target" {
  [[ "$(plain_view today:3 "$ITEMS")" == *$'Enter  Open the result\ns      Send to lisk-strategist…'* ]]
  [[ "$(plain_view today:4 "$ITEMS")" == *$'Enter  Open the text to send\ns      Revise: send to lisk-delivery…'* ]]
  [[ "$(plain_view today:5 "$ITEMS")" == *$'Enter  Open the briefing\ns      Send to the work Ask session, with this briefing…'* ]]
  [[ "$(plain_view today:6 "$ITEMS")" == *$'Enter  Open the failure\ns      Send to the Task\'s agent…'* ]]
  [[ "$(plain_view today:7 "$ITEMS")" == *"s      nothing to send on this item; use its keys below"* ]]
  [ "$(inbox key today:1 s "$ITEMS")" = "" ]
  [ "$(inbox key "$(inbox target today:1 "$ITEMS")" s "$ITEMS")" = reply ]
  [ "$(inbox key e-2b3c4d5e s "$ITEMS")" = ask-agent ]
  [ "$(inbox key w-7b8c9d0e s "$ITEMS")" = secretary ]
  # Below level 17 the host cannot type to the secretary: no s on a Work item.
  today_inbox 16 >"$TMPDIR/16.json"
  [ "$(inbox key w-7b8c9d0e s "$TMPDIR/16.json")" = "" ]
  [[ "$(plain_view today:8 "$TMPDIR/16.json")" == *"s      Send to the secretary needs muxa work level 17"* ]]
  [[ "$(plain_view w-7b8c9d0e "$ITEMS")" == *"P Pin   s Send to the secretary"* ]]
  [[ "$(plain_view w-7b8c9d0e "$TMPDIR/16.json")" != *"s Send to the secretary"* ]]
}

@test "Enter on a Today row opens a Task's review or menu, else moves to the target" {
  [ "$(inbox enter today:1 "$ITEMS")" = $'session:$8\ti-2c3d4e5f' ]
  [ "$(inbox enter today:2 "$ITEMS")" = $'session:$7\ti-0a1b2c3d' ]
  [ "$(inbox enter today:3 "$ITEMS")" = focus:i-9c0d1e2f ]
  [ "$(inbox enter today:6 "$ITEMS")" = focus:i-3d4e5f6a ]
  [ "$(inbox enter today:8 "$ITEMS")" = focus:w-7b8c9d0e ]
  [ "$(inbox enter today: "$ITEMS")" = "" ]
  # A closed target opens nothing.
  jq -c '.plans[0].top[0].targetState = "answered"' "$ITEMS" >"$TMPDIR/c.json"
  [ "$(inbox enter today:1 "$TMPDIR/c.json")" = "" ]
  [[ "$(plain_view today:1 "$TMPDIR/c.json")" != *"Enter "* ]]
}

@test "without overallRuns, Re-rank shows no count" {
  jq -c 'del(.assistant.overallRuns)' "$ITEMS" >"$TMPDIR/n.json"
  [[ "$(plain_view today:1 "$TMPDIR/n.json")" == *$'plan:  K/J move   x not today   t Re-rank\n'* ]]
}

@test "prefix+T: Enter on a Work row moves to it; s sends it to the secretary" {
  fake_cli
  attach
  key C-b T
  sleep 1
  # The cursor starts on today:; the Work item's row, today:8, is 8 down.
  key j j j j j j j j
  key s
  sleep 0.5
  key -l 'Ask legal about offer B'
  key Enter
  wait_for 'grep -q "^ask --root --workspace secretary --work w-7b8c9d0e --text Ask legal about offer B --json$" "$TMPDIR/bin/calls"'
  sleep 1
  key Enter
  key d
  wait_for 'grep -q "^work done w-7b8c9d0e --json$" "$TMPDIR/bin/calls"'
}
