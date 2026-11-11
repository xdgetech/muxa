#!/usr/bin/env bats
load helper

setup() { setup_server; }
teardown() { teardown_server; }

@test "help lists the keys the options set, and how to make a manual session" {
  tx set-option -g @agents_key_ask Q
  run "$ROOT/scripts/help.sh"
  [ "$status" -eq 0 ]
  out="$(printf '%s\n' "$output" | strip_ansi)"
  [[ "$out" == *"Anywhere: C-b, then"* ]]
  [[ "$out" == *"  Q              ask session"* ]]
  [[ "$out" == *"ask sessions (C-b Q)"* ]]
  [[ "$out" == *"new -s NAME -c DIR"* ]]
  [[ "$out" == *"  space          fold or unfold a section"* ]]
  [[ "$out" == *"a ask the assistant"* ]]
  [[ "$out" == *"  l d a          on a Work item: Later, Complete, ask its assistant"* ]]
}

@test "help lists the Inbox section keys" {
  run "$ROOT/scripts/help.sh"
  out="$(printf '%s\n' "$output" | strip_ansi)"
  [[ "$out" == *"  Tab S-Tab      the next or the previous section header"* ]]
  [[ "$out" == *"  Enter          on the Routine header: unfold or fold the briefings"* ]]
}

@test "help lists the Today keys of ADR 0049" {
  run "$ROOT/scripts/help.sh"
  out="$(printf '%s\n' "$output" | strip_ansi)"
  [[ "$out" == *"  P              on a Work item: pin it first, or unpin it"* ]]
  [[ "$out" == *"  t              Re-rank: re-rank the queue from what's open now (one overall run)"* ]]
  [[ "$out" == *"  o              on a Today row: go to its Work item or Inbox item"* ]]
}

@test "help lists what Enter and s do on a Today row (ADR 0059)" {
  run "$ROOT/scripts/help.sh"
  out="$(printf '%s\n' "$output" | strip_ansi)"
  [[ "$out" == *"A Today row names what to do: Review changes, Read result, Start..."* ]]
  [[ "$out" == *"  Enter          on a Today row: open a Task's review or menu, a Work item's menu, else go to its item"* ]]
  [[ "$out" == *"  Enter          on a Work item: its menu: open its Task, read its result, start a Task, ask..."* ]]
  [[ "$out" == *"  n              on a Work item: New Task form with its workspace and title, for the item"* ]]
  [[ "$out" == *"  s              on a Today row: send to whoever it names"* ]]
}

@test "help lists the Today and Policies keys of ADR 0050" {
  run "$ROOT/scripts/help.sh"
  out="$(printf '%s\n' "$output" | strip_ansi)"
  [[ "$out" == *"  K J            on a Today row: move it up or down"* ]]
  [[ "$out" == *"  x              on a Today row: Not today, with an optional reason"* ]]
  [[ "$out" == *"  y              the Policies list: add, edit or delete a policy; forget what it learned"* ]]
}
