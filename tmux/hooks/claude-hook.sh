#!/usr/bin/env bash
# Claude Code hook: maps a hook event (JSON on stdin) to the pane's agent state.
# Outside tmux it updates only a background job's pane (see agent_pane_in).
# Installed by scripts/install-claude-hooks.sh.
command -v jq >/dev/null || exit 0
. "$(dirname "$0")/../scripts/lib.sh"

input="$(cat)"
# Unit separator, not tab: read would merge empty tab-separated fields.
fields="$(printf '%s' "$input" | jq -r '[.hook_event_name, .session_id, .tool_name, .notification_type, .cwd] | map(. // "") | join("\u001f")' 2>/dev/null)" || exit 0
IFS=$'\037' read -r event sid tool ntype cwd <<EOF
$fields
EOF

pane="${TMUX_PANE:-}"
if [ -z "$pane" ]; then
  pane="$(agent_pane_in "$cwd")"
  [ -n "$pane" ] || exit 0
  # The job has its own session id; the pane keeps the one of its own agent.
  sid=""
fi

case "$event" in
  SessionStart) state=idle ;;
  UserPromptSubmit|PostToolUse|PostToolUseFailure) state=running ;;
  PreToolUse) if [ "$tool" = AskUserQuestion ]; then state=waiting; else state=running; fi ;;
  PermissionRequest|Elicitation) state=waiting ;;
  Notification)
    case "$ntype" in
      permission_prompt|elicitation_dialog|elicitation_url_dialog|agent_needs_input) state=waiting ;;
      *) exit 0 ;;
    esac ;;
  Stop) state="done" ;;
  StopFailure) state=error ;;
  SessionEnd) state="" ;;
  *) exit 0 ;;
esac

exec "$AGENTS_SCRIPTS/state.sh" set "$pane" "$state" "$sid"
