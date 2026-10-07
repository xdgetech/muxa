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
  [ "$(pane_opt "$PANE" @agent_state)" = done ]

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

# A background job's hooks run outside tmux; tmux reports macOS /tmp as /private/tmp.
bg_hook() { printf '%s' "$1" | env -u TMUX_PANE "$ROOT/hooks/claude-hook.sh"; }
pane_dir() { pane_opt "$PANE" pane_current_path; }

@test "a background job updates the agent pane in its directory" {
  hook '{"hook_event_name":"SessionStart","session_id":"own"}'
  bg_hook '{"hook_event_name":"UserPromptSubmit","session_id":"job","cwd":"'"$(pane_dir)"'"}'
  [ "$(pane_opt "$PANE" @agent_state)" = running ]
  [ "$(pane_opt "$PANE" @agent_session_id)" = own ]
}

@test "a background job changes nothing when several agent panes share its directory" {
  hook '{"hook_event_name":"SessionStart","session_id":"own"}'
  other="$(tx split-window -t base -c "$(pane_dir)" -P -F '#{pane_id}')"
  printf '%s' '{"hook_event_name":"SessionStart","session_id":"two"}' | TMUX_PANE="$other" "$ROOT/hooks/claude-hook.sh"
  wait_for "[ \"\$(pane_opt '$other' pane_current_path)\" = '$(pane_dir)' ]"
  bg_hook '{"hook_event_name":"UserPromptSubmit","session_id":"job","cwd":"'"$(pane_dir)"'"}'
  [ "$(pane_opt "$PANE" @agent_state)" = idle ]
  [ "$(pane_opt "$other" @agent_state)" = idle ]
}

@test "codex turn complete marks the pane done" {
  TMUX_PANE="$PANE" "$ROOT/hooks/codex-notify.sh" '{"type":"agent-turn-complete","turn-id":"1"}'
  [ "$(pane_opt "$PANE" @agent_state)" = done ]
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
  [[ "$status" == *"🙋 1 "* ]]
  [[ "$status" == *"⚡ 1 "* ]]
}

@test "a state change records when it happened" {
  before="$(date +%s)"
  "$ROOT/scripts/state.sh" set "$PANE" running
  at="$(pane_opt "$PANE" @agent_state_at)"
  [ "$at" -ge "$before" ]
  [ "$at" -le "$(date +%s)" ]
}

# notify.sh runs in the background; give it time to write.
notified() { sleep 0.5; cat "$TMPDIR/notified" 2>/dev/null; }

@test "notifies when the agent needs input, finishes, or fails" {
  printf '#!/bin/sh\necho "$1|$2" >>"%s/notified"\n' "$TMPDIR" >"$TMPDIR/rec"
  chmod +x "$TMPDIR/rec"
  tx set-option -g @agents_notify on
  tx set-option -g @agents_notify_cmd "$TMPDIR/rec"
  "$ROOT/scripts/state.sh" set "$PANE" idle
  "$ROOT/scripts/state.sh" set "$PANE" running
  "$ROOT/scripts/state.sh" set "$PANE" waiting
  "$ROOT/scripts/state.sh" set "$PANE" running
  "$ROOT/scripts/state.sh" set "$PANE" done
  "$ROOT/scripts/state.sh" set "$PANE" idle
  "$ROOT/scripts/state.sh" set "$PANE" error
  run notified
  [ "${#lines[@]}" -eq 3 ]
  [[ "$output" == *"base|needs your input"* ]]
  [[ "$output" == *"base|finished"* ]]
  [[ "$output" == *"base|failed"* ]]
}

@test "notifications follow the event list and the off switch" {
  printf '#!/bin/sh\necho "$1|$2" >>"%s/notified"\n' "$TMPDIR" >"$TMPDIR/rec"
  chmod +x "$TMPDIR/rec"
  tx set-option -g @agents_notify_cmd "$TMPDIR/rec"
  tx set-option -g @agents_notify on
  tx set-option -g @agents_notify_events waiting
  "$ROOT/scripts/state.sh" set "$PANE" error
  "$ROOT/scripts/state.sh" set "$PANE" waiting
  tx set-option -g @agents_notify off
  "$ROOT/scripts/state.sh" set "$PANE" running
  "$ROOT/scripts/state.sh" set "$PANE" waiting
  run notified
  [ "$output" = "base|needs your input" ]
}
