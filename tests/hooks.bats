#!/usr/bin/env bats
load helper

setup() { setup_server; PANE="$(tx display-message -p -t base '#{pane_id}')"; }
teardown() { teardown_server; }

hook() { printf '%s' "$1" | TMUX_PANE="$PANE" "$ROOT/hooks/claude-hook.sh"; }

@test "hook events map to pane states" {
  hook '{"hook_event_name":"UserPromptSubmit","session_id":"abc"}'
  [ "$(pane_opt "$PANE" @agent_state)" = running ]
  [ "$(pane_opt "$PANE" @agent_session_id)" = abc ]

  hook '{"hook_event_name":"PermissionRequest","session_id":"abc","tool_name":"Bash"}'
  [ "$(pane_opt "$PANE" @agent_state)" = waiting ]

  hook '{"hook_event_name":"PostToolUse","session_id":"abc","tool_name":"Bash"}'
  [ "$(pane_opt "$PANE" @agent_state)" = running ]

  hook '{"hook_event_name":"PreToolUse","session_id":"abc","tool_name":"AskUserQuestion"}'
  [ "$(pane_opt "$PANE" @agent_state)" = waiting ]

  hook '{"hook_event_name":"Stop","session_id":"abc"}'
  [ "$(pane_opt "$PANE" @agent_state)" = idle ]

  hook '{"hook_event_name":"StopFailure","session_id":"abc","error_type":"rate_limit"}'
  [ "$(pane_opt "$PANE" @agent_state)" = error ]

  hook '{"hook_event_name":"SessionEnd","session_id":"abc"}'
  [ -z "$(pane_opt "$PANE" @agent_state)" ]
}

@test "a permission notification is waiting even without a tool name" {
  hook '{"hook_event_name":"Notification","session_id":"abc","notification_type":"permission_prompt"}'
  [ "$(pane_opt "$PANE" @agent_state)" = waiting ]
}

@test "an idle notification does not change the state" {
  hook '{"hook_event_name":"UserPromptSubmit","session_id":"abc"}'
  hook '{"hook_event_name":"Notification","session_id":"abc","notification_type":"idle_prompt"}'
  [ "$(pane_opt "$PANE" @agent_state)" = running ]
}

@test "outside tmux the hook does nothing" {
  printf '%s' '{"hook_event_name":"Stop"}' | env -u TMUX_PANE "$ROOT/hooks/claude-hook.sh"
  [ -z "$(pane_opt "$PANE" @agent_state)" ]
}

@test "codex turn complete marks the pane idle" {
  TMUX_PANE="$PANE" "$ROOT/hooks/codex-notify.sh" '{"type":"agent-turn-complete","turn-id":"1"}'
  [ "$(pane_opt "$PANE" @agent_state)" = idle ]
}

@test "status bar counts sessions by their most urgent pane" {
  tx new-session -d -s two
  tx split-window -t two
  "$ROOT/scripts/state.sh" set "$PANE" running
  p1="$(tx list-panes -t two -F '#{pane_id}' | sed -n 1p)"
  p2="$(tx list-panes -t two -F '#{pane_id}' | sed -n 2p)"
  "$ROOT/scripts/state.sh" set "$p1" running
  "$ROOT/scripts/state.sh" set "$p2" waiting
  status="$(tx show-options -gqv @agents_status)"
  [[ "$status" == *"◐ 1 "* ]]
  [[ "$status" == *"● 1 "* ]]
}
