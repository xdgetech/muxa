#!/usr/bin/env bats
# Proposed Tasks in tmux (ADR 0060). A result item that proposes a Task
# offers a Start Task, or n beside a hand-off; a new-task item from the
# secretary offers a Start Task and s to the secretary. A plan item's a
# approves the plan, r and s keep planning. The data is the contract's.
load helper

FIXTURE="$ROOT/../protocol/fixtures/task-proposals/messages.jsonl"

setup() {
  setup_server
  JSON="$TMPDIR/inbox.json"
  jq -c 'select(.type == "inbox-list") | {work: 18, items, entries: [], runs: []}' "$FIXTURE" >"$JSON"
  PLAN="$TMPDIR/plan.json"
  jq -c '{work: 18, entries: [], runs: [], items: [{id: "i-7b8c9d0e", kind: "ask", state: "open",
      question: "Claude wants to leave plan mode", workspace: "muxa", title: "Fix the crash",
      createdAt: 1762850000, task: "t-7f3a2c", session: "$4", pane: "%9",
      prompt: {entry: "toolu_1", kind: "plan", options: [{label: "Keep planning", keys: ["Escape"], cancel: true}]},
      answers: ["approve", "reject", "reply", "later"]}]}' -n >"$PLAN"
}
teardown() { teardown_server; }

inbox() { "$ROOT/scripts/inbox.sh" "$@"; }
plain() { "$@" | strip_ansi; }

@test "a result item that proposes a Task starts it with a" {
  out="$(plain inbox view i-4e5f6a7b "$JSON")"
  [[ "$out" == *"Proposed Task: Fix the Inbox crash on an empty result"* ]]
  [[ "$out" == *"In: muxa · repo muxa"* ]]
  [[ "$out" == *"Guard the empty summary in the result item and add a test."* ]]
  [[ "$out" == *"starts it in plan mode"* ]]
  [[ "$out" == *"a Start Task   s Send"* ]]
  [ "$(inbox key i-4e5f6a7b a "$JSON")" = approve ]
  [ -z "$(inbox key i-4e5f6a7b n "$JSON")" ]
}

@test "beside a hand-off, a hands off and n starts the Task" {
  out="$(plain inbox view i-5f6a7b8c "$JSON")"
  [[ "$out" == *"a Hand off to muxa-review   s Send   n Start Task"* ]]
  [[ "$out" == *"n starts the Task instead"* ]]
  [ "$(inbox key i-5f6a7b8c a "$JSON")" = approve ]
  [ "$(inbox key i-5f6a7b8c n "$JSON")" = start-task ]
}

@test "the secretary's new-task item starts a Task, and s goes to the secretary" {
  out="$(plain inbox view i-6a7b8c9d "$JSON")"
  [[ "$out" == *"🛠"* ]]
  [[ "$out" == *"Proposed Task: Fix the Inbox crash on an empty result"* ]]
  [[ "$out" == *"a Start Task   s Send to the secretary"* ]]
  [ "$(inbox key i-6a7b8c9d a "$JSON")" = approve ]
  [ "$(inbox key i-6a7b8c9d s "$JSON")" = reply ]
  [[ "$(plain inbox list "$JSON" | grep '^i-6a7b8c9d')" == *"🛠 The secretary proposes a Task in muxa"* ]]
}

@test "a started proposal offers o Open its Task" {
  jq -c '.items[0] |= (.state = "answered" | .answers = [] | .task = "t-7f3a2c")' "$JSON" >"$JSON.tmp"
  out="$(plain inbox view i-4e5f6a7b "$JSON.tmp")"
  [[ "$out" == *"o Open its Task"* ]]
  [[ "$out" != *"a starts the Task"* ]]
}

@test "a plan item approves the plan; r and s keep planning" {
  out="$(plain inbox view i-7b8c9d0e "$PLAN")"
  [[ "$out" == *"a Approve plan (edits one by one)   r Keep planning   s Keep planning & Send"* ]]
  [[ "$out" == *"asks before each edit"* ]]
  [[ "$out" == *"o Open Session"* ]]
  [ "$(inbox key i-7b8c9d0e a "$PLAN")" = approve ]
  [ "$(inbox key i-7b8c9d0e r "$PLAN")" = reject ]
  [ "$(inbox key i-7b8c9d0e s "$PLAN")" = reply ]
}
