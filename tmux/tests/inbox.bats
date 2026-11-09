#!/usr/bin/env bats
# The Inbox of ADR 0033: the sidebar section and prefix + T. The items are the
# contract's own fixture, so the plugin reads what muxa sends, reply included
# (ADR 0033 §4a).
load helper

setup() {
  setup_server
  ITEMS="$TMPDIR/inbox.json"
  jq -c 'select(.type == "inbox-list") | {work: 3, items, entries: [], runs: []}' \
    "$ROOT/../protocol/fixtures/items/messages.jsonl" >"$ITEMS"
}
teardown() { kill "${CLIENT_PID:-}" 2>/dev/null || true; teardown_server; }

inbox() { "$ROOT/scripts/inbox.sh" "$@"; }

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

@test "the list holds open and later items, in muxa's order, and no closed ones" {
  run bash -c "'$ROOT/scripts/inbox.sh' list '$ITEMS' | cut -f1"
  [ "$status" -eq 0 ]
  [ "$output" = "$(printf '%s\n' i-0a1b2c3d i-2c3d4e5f i-3d4e5f6a e-1a2b3c4d)" ]
}

@test "an item shows its question, summary, numbered evidence and its keys" {
  run bash -c "'$ROOT/scripts/inbox.sh' view i-2c3d4e5f '$ITEMS' | sed \$'s/\x1b\\\\[[0-9;]*m//g'"
  [ "$status" -eq 0 ]
  [[ "$output" == *"Ready for review: Reduce GA cost"* ]]
  [[ "$output" == *"Moved the GA4 export to daily batches. Tests pass."* ]]
  [[ "$output" == *"1  Changes: 4 files, +120 −40 (branch)"* ]]
  [[ "$output" == *"2  docs/plan.md"* ]]
  [[ "$output" == *"3  docs/chart.png"* ]]
  [[ "$output" == *"4  PR #817  https://github.com/example/app/pull/817"* ]]
  [[ "$output" == *$'l Later   d ✓ Done   c Complete Task   o Open Session\ns Send\nspace/b scroll   v read all   q close'* ]]
}

@test "labels name what an answer does, as in the apps: triage first, then the answers" {
  keys() { inbox view "$1" "$ITEMS" | sed $'s/\x1b\\[[0-9;]*m//g' | tail -n 3 | sed '$d'; }
  [ "$(keys i-0a1b2c3d)" = "$(printf 'l Later   o Open Session\na Allow   r Deny   s Deny & Send')" ]
  [ "$(keys i-3d4e5f6a)" = "$(printf 'l Later   d ✓ Done   x Drop Task\na Retry   s Send')" ]
  [ "$(keys e-1a2b3c4d)" = "$(printf 'l Later\na Approve   r Reject')" ]
}

@test "a key answers with the item's own answers, a number opens evidence" {
  [ "$(inbox key i-2c3d4e5f a "$ITEMS")" = "" ]
  [ "$(inbox key i-2c3d4e5f c "$ITEMS")" = accept ]
  [ "$(inbox key i-2c3d4e5f d "$ITEMS")" = keep-open ]
  [ "$(inbox key i-2c3d4e5f r "$ITEMS")" = "" ]
  [ "$(inbox key i-2c3d4e5f s "$ITEMS")" = reply ]
  [ "$(inbox key i-0a1b2c3d s "$ITEMS")" = reply ]
  [ "$(inbox key i-3d4e5f6a s "$ITEMS")" = reply ]
  [ "$(inbox key e-1a2b3c4d s "$ITEMS")" = "" ]
  [ "$(inbox key i-2c3d4e5f 2 "$ITEMS")" = open:2 ]
  [ "$(inbox key i-2c3d4e5f x "$ITEMS")" = "" ]
  [ "$(inbox key i-0a1b2c3d a "$ITEMS")" = approve ]
  [ "$(inbox key i-0a1b2c3d r "$ITEMS")" = reject ]
  [ "$(inbox key i-3d4e5f6a a "$ITEMS")" = retry ]
  [ "$(inbox key i-3d4e5f6a x "$ITEMS")" = drop ]
  [ "$(inbox key i-3d4e5f6a d "$ITEMS")" = dismiss ]
  [ "$(inbox key i-3d4e5f6a p "$ITEMS")" = "" ]
  [ "$(inbox key i-3d4e5f6a n "$ITEMS")" = "" ]
  [ "$(inbox key e-1a2b3c4d r "$ITEMS")" = reject ]
}

@test "a question's numbers choose an option" {
  jq -c '.items[0].prompt = {entry: "toolu_02", kind: "question", question: "Which database?",
           options: [{label: "Postgres", keys: ["1"]}, {label: "SQLite", keys: ["2"]}]}
         | .items[0].answers = ["choose", "later"]' "$ITEMS" >"$TMPDIR/q.json"
  [ "$(inbox key i-0a1b2c3d 2 "$TMPDIR/q.json")" = choose:1 ]
  [ "$(inbox key i-0a1b2c3d a "$TMPDIR/q.json")" = "" ]
  run bash -c "'$ROOT/scripts/inbox.sh' view i-0a1b2c3d '$TMPDIR/q.json' | sed \$'s/\x1b\\\\[[0-9;]*m//g'"
  [[ "$output" == *"Which database?"* ]]
  [[ "$output" == *"2  SQLite"* ]]
}

@test "the sidebar section counts the open items and shows the first ones" {
  fake_cli
  inbox cache --force
  run bash -c "'$ROOT/scripts/inbox.sh' rows | cut -f1"
  [ "$output" = "$(printf '%s\n' I: i-0a1b2c3d i-2c3d4e5f i-3d4e5f6a | sed '2,$s/^/i:/')" ]
  [[ "$(inbox rows | head -n 1)" == *"Inbox"*" 4"* ]]
  tx set-option -g @agents_inbox_rows 1
  [ "$(inbox rows | wc -l | tr -d ' ')" = 2 ]
}

@test "the section is hidden on an older muxa and with muxa off" {
  fake_cli
  jq -c '.work = 1' "$ITEMS" >"$ITEMS.tmp" && mv "$ITEMS.tmp" "$ITEMS"
  inbox cache --force
  [ -z "$(inbox rows)" ]
  jq -c '.work = 3' "$ITEMS" >"$ITEMS.tmp" && mv "$ITEMS.tmp" "$ITEMS"
  inbox cache --force
  [ -n "$(inbox rows)" ]
  tx set-option -g @agents_muxa off
  [ -z "$(inbox rows)" ]
}

@test "the cache asks muxa once per interval, and again when forced" {
  fake_cli
  tx set-option -g @agents_inbox_refresh 60
  inbox cache
  inbox cache
  [ "$(wc -l <"$TMPDIR/bin/calls" | tr -d ' ')" = 1 ]
  inbox cache --force
  [ "$(wc -l <"$TMPDIR/bin/calls" | tr -d ' ')" = 2 ]
  [ "$(head -n 1 "$TMPDIR/bin/calls")" = "inbox --json" ]
}

@test "prefix+T: a on an ask allows it" {
  fake_cli
  attach
  key C-b T
  sleep 1
  key a
  wait_for 'grep -q "^inbox answer i-0a1b2c3d approve --json$" "$TMPDIR/bin/calls"'
}

@test "prefix+T: a never completes a Task; c does" {
  fake_cli
  attach
  key C-b T
  sleep 1
  key j
  key a
  # a does nothing here; the popup lists the Inbox again before it takes a key.
  sleep 1
  key c
  wait_for 'grep -q "^inbox answer i-2c3d4e5f accept --json$" "$TMPDIR/bin/calls"'
  [ "$(grep -c "^inbox answer" "$TMPDIR/bin/calls")" = 1 ]
}

@test "prefix+T: l moves the item down at once, with no time" {
  fake_cli
  attach
  key C-b T
  sleep 1
  key j
  key l
  wait_for 'grep -q "^inbox answer i-2c3d4e5f later --json$" "$TMPDIR/bin/calls"'
}

@test "prefix+T: s without a session asks the workspace's agent with a text" {
  fake_cli
  jq -c '.items[1].answers = ["accept", "ask-agent", "keep-open", "later"]' "$ITEMS" >"$ITEMS.tmp" && mv "$ITEMS.tmp" "$ITEMS"
  [ "$(inbox key i-2c3d4e5f s "$ITEMS")" = ask-agent ]
  [[ "$(inbox view i-2c3d4e5f "$ITEMS")" == *"s Ask Agent"* ]]
  attach
  key C-b T
  sleep 1
  key j
  key s
  tx send-keys -K -c "$C" -l "Put this in Notion"
  key Enter
  wait_for 'grep -q "^inbox answer i-2c3d4e5f ask-agent --text Put this in Notion --json$" "$TMPDIR/bin/calls"'
}

@test "prefix+T: s on a review asks for the text and sends it with reply" {
  fake_cli
  attach
  key C-b T
  sleep 1
  key j
  key s
  tx send-keys -K -c "$C" -l "Also update the README"
  key Enter
  wait_for 'grep -q "^inbox answer i-2c3d4e5f reply --text Also update the README --json$" "$TMPDIR/bin/calls"'
}

@test "prefix+T: s on an ask sends the text with reply" {
  fake_cli
  attach
  key C-b T
  sleep 1
  key s
  sleep 0.5
  tx send-keys -K -c "$C" -l "Run plan first"
  key Enter
  wait_for 'grep -q "^inbox answer i-0a1b2c3d reply --text Run plan first --json$" "$TMPDIR/bin/calls"'
}

@test "prefix+T: v shows the whole item in the pager" {
  fake_cli
  printf '#!/usr/bin/env bash\ncat >"%s/paged"\n' "$TMPDIR" >"$TMPDIR/bin/pager"
  chmod +x "$TMPDIR/bin/pager"
  tx set-environment -g PAGER "$TMPDIR/bin/pager"
  attach
  key C-b T
  sleep 1
  key j
  key v
  wait_for 'grep -q "Moved the GA4 export to daily batches" "$TMPDIR/paged"'
  wait_for 'grep -q "^inbox read i-2c3d4e5f" "$TMPDIR/bin/calls"'
}

@test "prefix+T: Enter goes to the item's session" {
  tx new-session -d -s target
  sid="$(tx display-message -p -t =target: '#{session_id}')"
  jq -c --arg sid "$sid" '.items[0].session = $sid' "$ITEMS" >"$ITEMS.tmp" && mv "$ITEMS.tmp" "$ITEMS"
  fake_cli
  attach
  key C-b T
  sleep 1
  key Enter
  wait_for '[ "$(tx display-message -p -c "$C" "#{session_name}")" = target ]'
  wait_for 'grep -q "^inbox read i-0a1b2c3d" "$TMPDIR/bin/calls"'
}

@test "the sidebar shows the Inbox above the sessions" {
  fake_cli
  attach
  sb="$(tx list-panes -t base: -F '#{pane_id} #{@agents_sidebar_pane}' | awk '$2 == 1 { print $1 }')"
  wait_for "tx capture-pane -p -t '$sb' | grep -q 'Inbox 4'"
  wait_for "tx capture-pane -p -t '$sb' | grep -q 'Allow Bash'"
}

# A work:8 Inbox (ADR 0046): the contract's Work items and Captures follow the
# items. The fixture's level 9 list (with plans) is plan_inbox's.
work_inbox() {
  jq -c --slurpfile items "$ITEMS" 'select(.type == "inbox-list" and .plans == null) |
    {work: 8, items: $items[0].items, entries: [], runs: [], workItems: .work, captures, requests}' \
    "$ROOT/../protocol/fixtures/assistant/messages.jsonl"
}

@test "on work:8 the list ends with the open Work items, then the unsorted Captures" {
  work_inbox >"$TMPDIR/w.json"
  run bash -c "'$ROOT/scripts/inbox.sh' list '$TMPDIR/w.json' | cut -f1 | tail -n 5"
  [ "$status" -eq 0 ]
  [ "$output" = "$(printf '%s\n' w-6a7b8c9d w-5f6a7b8c w-7b8c9d0e w-8c9d0e1f c-4e5f6a7b)" ]
  run bash -c "'$ROOT/scripts/inbox.sh' list '$TMPDIR/w.json' | grep '^w-8c9d0e1f' | sed \$'s/\x1b\\\\[[0-9;]*m//g'"
  [[ "$output" == *"Read the Rust async book  open · "*" · later"* ]]
}

@test "a Work item and a Capture show their own keys, and nothing else answers" {
  work_inbox >"$TMPDIR/w.json"
  run bash -c "'$ROOT/scripts/inbox.sh' view w-6a7b8c9d '$TMPDIR/w.json' | sed \$'s/\x1b\\\\[[0-9;]*m//g'"
  [[ "$output" == *"waiting on decision · work"* ]]
  [[ "$output" == *"l Later   d Complete   a Ask Assistant"* ]]
  [ "$(inbox key w-6a7b8c9d l "$TMPDIR/w.json")" = work:later ]
  [ "$(inbox key w-6a7b8c9d d "$TMPDIR/w.json")" = work:done ]
  [ "$(inbox key w-6a7b8c9d a "$TMPDIR/w.json")" = assist ]
  [ "$(inbox key w-6a7b8c9d r "$TMPDIR/w.json")" = "" ]
  [ "$(inbox key c-4e5f6a7b a "$TMPDIR/w.json")" = "" ]
  [ "$(inbox key c-4e5f6a7b w "$TMPDIR/w.json")" = sort:new ]
  [ "$(inbox key c-4e5f6a7b n "$TMPDIR/w.json")" = sort:note ]
  [ "$(inbox key c-4e5f6a7b l "$TMPDIR/w.json")" = "" ]
}

@test "a Capture's control characters are not shown" {
  work_inbox | jq -c '.captures[0].text = "Ring \u001b[31mred\u001b[0m\nsecond"' >"$TMPDIR/w.json"
  run bash -c "'$ROOT/scripts/inbox.sh' list '$TMPDIR/w.json' | grep '^c-' | cut -f2"
  [[ "$output" == *"Ring [31mred[0m  "* ]]
  [[ "$output" != *"second"* ]]
}

@test "prefix+T: l on a Work item moves it down through muxa work later" {
  fake_cli
  work_inbox >"$ITEMS.tmp" && mv "$ITEMS.tmp" "$ITEMS"
  jq -c '.items = []' "$ITEMS" >"$ITEMS.tmp" && mv "$ITEMS.tmp" "$ITEMS"
  attach
  key C-b T
  sleep 1
  key l
  wait_for 'grep -q "^work later w-6a7b8c9d --json$" "$TMPDIR/bin/calls"'
}

@test "a Work row shows its newest request's delivery, and nothing when never asked" {
  work_inbox >"$TMPDIR/w.json"
  run bash -c "'$ROOT/scripts/inbox.sh' list '$TMPDIR/w.json' | grep '^w-' | cut -f2 | sed \$'s/\x1b\\\\[[0-9;]*m//g'"
  [[ "${lines[0]}" == *"waiting on decision · asked: resulted · "* ]]
  [[ "${lines[1]}" == *"in progress · asked: processing · "* ]]
  # Its newest request is the last it lists.
  [[ "${lines[2]}" == *"open · asked: timed out · "* ]]
  [[ "${lines[3]}" != *"asked"* ]]
}

@test "a Work item shows its newest Result's summary, as plain text" {
  work_inbox | jq -c '.requests[0].results[0].summary = "Merge \u001b[31msteps\u001b[0m 3 to 5."' >"$TMPDIR/w.json"
  run bash -c "'$ROOT/scripts/inbox.sh' view w-6a7b8c9d '$TMPDIR/w.json'"
  [[ "$output" == *"Assistant: compare v2 · resulted · work"* ]]
  [[ "$output" == *"Result v2"* ]]
  [[ "$output" == *"Merge [31msteps[0m 3 to 5."* ]]
  [[ "$output" != *$'\x1b[31m'* ]]
  [[ "$output" == *"muxa-cli assist show w-6a7b8c9d"* ]]
  [[ "$output" != *"The guide has 9 steps."* ]]
  run bash -c "'$ROOT/scripts/inbox.sh' view w-8c9d0e1f '$TMPDIR/w.json'"
  [[ "$output" != *"Assistant:"* ]]
  [[ "$output" != *"Result"* ]]
}

@test "prefix+T: a on a Work item asks its assistant through muxa assist" {
  fake_cli
  work_inbox >"$ITEMS.tmp" && mv "$ITEMS.tmp" "$ITEMS"
  jq -c '.items = []' "$ITEMS" >"$ITEMS.tmp" && mv "$ITEMS.tmp" "$ITEMS"
  attach
  key C-b T
  sleep 1
  key a
  wait_for 'grep -q "^assist w-6a7b8c9d --json$" "$TMPDIR/bin/calls"'
}

# A work:9 Inbox (ADR 0047): the contract's plans, overall assistant, and sort
# and delegate items.
plan_inbox() {
  jq -c 'select(.type == "inbox-list" and .plans) |
    {work: 9, items, entries: [], runs: [], workItems: .work, captures, requests, plans, assistant}' \
    "$ROOT/../protocol/fixtures/assistant/messages.jsonl"
}

plain_list() { inbox list "$1" | sed $'s/\x1b\\[[0-9;]*m//g'; }
plain_view() { inbox view "$1" "$2" | sed $'s/\x1b\\[[0-9;]*m//g'; }

@test "on work:9 Today comes first: the plan's rows, then the Due soon line" {
  plan_inbox >"$TMPDIR/p.json"
  run bash -c "'$ROOT/scripts/inbox.sh' list '$TMPDIR/p.json' | cut -f1 | head -n 7"
  [ "$output" = "$(printf '%s\n' today: today:1 today:2 today:3 today:due i-1a2b3c4d i-2b3c4d5e)" ]
  out="$(plain_list "$TMPDIR/p.json")"
  [[ "$out" == *$'today:1\t  1. Prepare the quarterly report  60–120 min · moved'* ]]
  [[ "$out" == *$'today:3\t  3. Try the tool for business B  1–5 min · new'* ]]
  [[ "$out" == *"Due soon, moved down by you: Renew the passport"* ]]
  [[ "$out" == *"Prepare the quarterly report  open · "*" · pinned · due 2025-11-12 07:00Z · work"* ]]
}

@test "a Today row shows why, next, time and basis, and opens its target" {
  plan_inbox >"$TMPDIR/p.json"
  out="$(plain_view today:2 "$TMPDIR/p.json")"
  [[ "$out" == *"2. Compare vendor offers"* ]]
  [[ "$out" == *"Why: The offers end Friday."* ]]
  [[ "$out" == *"Next: Approve the request to work."* ]]
  [[ "$out" == *"Time: 5–15 min, about 10"* ]]
  [[ "$out" == *"due: The offer ends Friday  https://example.com/offer"* ]]
  [[ "$out" == *"Which region the contract covers"* ]]
  [[ "$out" == *"o Open   p Plan Now"* ]]
  [ "$(inbox target today:1 "$TMPDIR/p.json")" = w-0b1c2d3e ]
  [ "$(inbox target today:3 "$TMPDIR/p.json")" = i-1a2b3c4d ]
  [ "$(inbox target today:due "$TMPDIR/p.json")" = w-8c9d0e1f ]
  out="$(plain_view today: "$TMPDIR/p.json")"
  [[ "$out" == *"Unanswered requests: q-2c3d4e5f"* ]]
  [[ "$out" == *"Stale results: q-1b2c3d4e"* ]]
  [[ "$out" == *"Left the plan: w-5f6a7b8c"* ]]
}

@test "a failed newest plan shows its reason, and the rows stay the last result's" {
  plan_inbox | jq -c --slurpfile m "$ROOT/../protocol/fixtures/assistant/messages.jsonl" \
    '.plans = [($m[] | select(.type == "plan-saved" and .entry.delivery == "failed") | .entry)] + .plans' \
    >"$TMPDIR/p.json"
  [[ "$(plain_list "$TMPDIR/p.json" | head -n 1)" == *"Today  last plan failed"* ]]
  [ "$(inbox target today:1 "$TMPDIR/p.json")" = w-0b1c2d3e ]
  out="$(plain_view today: "$TMPDIR/p.json")"
  [[ "$out" == *"p-2c3d4e5f · failed"* ]]
  [[ "$out" == *"The run did not start: cannot reach tmux on the host"* ]]
}

@test "Today is absent below work:9, and without plans or an overall assistant" {
  plan_inbox | jq -c '.work = 8' >"$TMPDIR/p.json"
  [[ "$(inbox list "$TMPDIR/p.json" | cut -f1)" != *today* ]]
  [ "$(inbox key w-0b1c2d3e P "$TMPDIR/p.json")" = "" ]
  [ "$(inbox key w-0b1c2d3e p "$TMPDIR/p.json")" = "" ]
  plan_inbox | jq -c '.plans = [] | del(.assistant)' >"$TMPDIR/p.json"
  [[ "$(inbox list "$TMPDIR/p.json" | cut -f1)" != *today* ]]
  plan_inbox | jq -c '.plans = []' >"$TMPDIR/p.json"
  [[ "$(plain_list "$TMPDIR/p.json" | head -n 1)" == *"Today  no plan yet · p Plan Now"* ]]
}

@test "p plans now from any row, P pins or unpins a Work item" {
  plan_inbox >"$TMPDIR/p.json"
  [ "$(inbox key today:1 p "$TMPDIR/p.json")" = plan:now ]
  [ "$(inbox key i-1a2b3c4d p "$TMPDIR/p.json")" = plan:now ]
  [ "$(inbox key w-0b1c2d3e P "$TMPDIR/p.json")" = work:unpin ]
  [ "$(inbox key w-7b8c9d0e P "$TMPDIR/p.json")" = work:pin ]
  [ "$(inbox key today:1 a "$TMPDIR/p.json")" = "" ]
  [[ "$(plain_view w-0b1c2d3e "$TMPDIR/p.json")" == *"a Ask Assistant   P Unpin"* ]]
  [[ "$(plain_view w-7b8c9d0e "$TMPDIR/p.json")" == *"a Ask Assistant   P Pin"* ]]
}

@test "sort and delegate items show their proposal and take approve, reject and later" {
  plan_inbox >"$TMPDIR/p.json"
  for id in i-1a2b3c4d i-2b3c4d5e; do
    [ "$(inbox key "$id" a "$TMPDIR/p.json")" = approve ]
    [ "$(inbox key "$id" r "$TMPDIR/p.json")" = reject ]
    [ "$(inbox key "$id" l "$TMPDIR/p.json")" = later ]
    [[ "$(plain_view "$id" "$TMPDIR/p.json")" == *$'l Later\na Approve   r Reject'* ]]
  done
  out="$(plain_view i-2b3c4d5e "$TMPDIR/p.json")"
  [[ "$out" == *"To: work · compare"* ]]
  [[ "$out" == *"Ask: Prices and contract terms of the two offers"* ]]
  [[ "$out" == *"Why: Due Friday, and the last request was not confirmed."* ]]
  [[ "$(plain_view i-1a2b3c4d "$TMPDIR/p.json")" == *"c-4e5f6a7b: a new Work item in work: Try the tool for business B"* ]]
}

@test "prefix+T: p asks for the minutes and plans now; o on a Today row moves to it, P pins" {
  fake_cli
  plan_inbox >"$ITEMS"
  attach
  key C-b T
  sleep 1
  key p
  key 4 5 Enter
  wait_for 'grep -q "^plan now --minutes 45 --json$" "$TMPDIR/bin/calls"'
  sleep 1
  key j j o
  key P
  wait_for 'grep -q "^work pin w-7b8c9d0e --json$" "$TMPDIR/bin/calls"'
}
