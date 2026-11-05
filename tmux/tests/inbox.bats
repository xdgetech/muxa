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
  [[ "$output" == *"a Complete Task   s Send   p ✓ Done   l Later (move down)   o open session   space/b scroll   v read all   q close"* ]]
}

@test "labels name what an answer does: an ask allows or denies, a proposal approves" {
  strip() { sed $'s/\x1b\\[[0-9;]*m//g'; }
  [[ "$(inbox view i-0a1b2c3d "$ITEMS" | strip | tail -n 1)" == "a Allow   r Deny   s Deny & send   l Later (move down)"* ]]
  [[ "$(inbox view i-3d4e5f6a "$ITEMS" | strip | tail -n 1)" == "a Retry   s Send   x Drop Task   n ✓ Done   l Later (move down)"* ]]
  [[ "$(inbox view e-1a2b3c4d "$ITEMS" | strip | tail -n 1)" == "a Approve   r Reject   l Later (move down)"* ]]
}

@test "a key answers with the item's own answers, a number opens evidence" {
  [ "$(inbox key i-2c3d4e5f a "$ITEMS")" = accept ]
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
  [ "$(inbox key i-3d4e5f6a n "$ITEMS")" = dismiss ]
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

@test "prefix+T: a on the selected item sends its main answer" {
  fake_cli
  attach
  key C-b T
  sleep 1
  key j
  key a
  wait_for 'grep -q "^inbox answer i-2c3d4e5f accept --json$" "$TMPDIR/bin/calls"'
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

@test "prefix+T: g asks the workspace's agent with a text, and the item stays" {
  fake_cli
  jq -c '.items[1].answers += ["ask-agent"]' "$ITEMS" >"$ITEMS.tmp" && mv "$ITEMS.tmp" "$ITEMS"
  [ "$(inbox key i-2c3d4e5f g "$ITEMS")" = ask-agent ]
  attach
  key C-b T
  sleep 1
  key j
  key g
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
