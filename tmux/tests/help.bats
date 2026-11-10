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

@test "help lists the Today keys of ADR 0049" {
  run "$ROOT/scripts/help.sh"
  out="$(printf '%s\n' "$output" | strip_ansi)"
  [[ "$out" == *"  P              on a Work item: pin it first, or unpin it"* ]]
  [[ "$out" == *"  t              ask the overall assistant for a plan now"* ]]
  [[ "$out" == *"  o              on a Today row: go to its Work item or Inbox item"* ]]
}

@test "help lists the Today and Policies keys of ADR 0050" {
  run "$ROOT/scripts/help.sh"
  out="$(printf '%s\n' "$output" | strip_ansi)"
  [[ "$out" == *"  K J            on a Today row: move it up or down"* ]]
  [[ "$out" == *"  x              on a Today row: Not today, with an optional reason"* ]]
  [[ "$out" == *"  y              the Policies list: add, edit or delete a policy; forget what it learned"* ]]
}
