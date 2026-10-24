#!/usr/bin/env bats
# The PreToolUse guard blocks tmux commands that reach the default server.

GUARD="$BATS_TEST_DIRNAME/../../scripts/hooks/guard-tmux.py"

guard() {
  jq -cn --arg c "$1" '{tool_name: "Bash", tool_input: {command: $c}}' | python3 -I "$GUARD"
}

@test "blocks tmux commands on the default server" {
  local c
  for c in 'tmux ls' 'tmux list-sessions' 'cd x && tmux attach' 'FOO=1 tmux ls' \
    "bash -lc 'tmux ls'" 'echo $(tmux display -p x)' 'echo `tmux ls`' \
    '/opt/homebrew/bin/tmux ls' 'tmux -f /dev/null new -d' 'env tmux ls'; do
    run guard "$c"
    [ "$status" -eq 2 ] || { echo "not blocked: $c"; return 1; }
    [[ "$output" == *"muxa guard"* ]]
  done
}

@test "allows isolated servers, version checks and non-tmux commands" {
  local c
  for c in 'tmux -L muxa-test ls' 'tmux -S /tmp/s ls' 'tmux -uL x ls' \
    'tmux -f /dev/null -L x new -d' 'tmux -V' 'echo tmux' 'grep tmux file' \
    'git commit -m "fix tmux ls"' 'scripts/tmux-sandbox.sh up' 'ls | grep tmux' \
    "echo 'unbalanced"; do
    run guard "$c"
    [ "$status" -eq 0 ] || { echo "blocked: $c"; return 1; }
  done
}

@test "ignores malformed hook input" {
  run bash -c "echo 'not json' | python3 -I '$GUARD'"
  [ "$status" -eq 0 ]
}
