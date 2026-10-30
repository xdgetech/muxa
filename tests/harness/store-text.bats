#!/usr/bin/env bats
# The store listing check passes on store/ and catches each rule it enforces.

ROOT="$BATS_TEST_DIRNAME/../.."
CHECK="$ROOT/scripts/check-store-text.sh"

setup() {
  copy="$BATS_TEST_TMPDIR/store"
  cp -R "$ROOT/store" "$copy"
}

@test "the checked-in listings pass" {
  run "$CHECK"
  [ "$status" -eq 0 ]
  [[ "$output" == *"store text: ok"* ]]
}

@test "a field over its limit fails, counted in characters" {
  printf 'Muxa: tmuxのAIエージェントをどこでもいつでも使うよ\n' >"$copy/apple/ja/name.txt"
  run "$CHECK" "$copy"
  [ "$status" -eq 1 ]
  [[ "$output" == *"apple/ja/name.txt has 31 characters, more than 30"* ]]
}

@test "an empty or missing field fails" {
  : >"$copy/play/en-US/title.txt"
  rm "$copy/apple/en-US/keywords.txt"
  run "$CHECK" "$copy"
  [ "$status" -eq 1 ]
  [[ "$output" == *"play/en-US/title.txt is empty"* ]]
  [[ "$output" == *"apple/en-US/keywords.txt is missing"* ]]
}

@test "listing text may not say demo or trial, but the review notes may" {
  printf 'Start a free trial.\n' >>"$copy/play/en-US/full_description.txt"
  printf 'デモをどうぞ。\n' >>"$copy/apple/ja/promotional_text.txt"
  run "$CHECK" "$copy"
  [ "$status" -eq 1 ]
  [[ "$output" == *"play/en-US/full_description.txt says demo or trial"* ]]
  [[ "$output" == *"apple/ja/promotional_text.txt says demo or trial"* ]]
  [[ "$output" != *"review_information"* ]]
}
