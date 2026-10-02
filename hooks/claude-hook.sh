#!/usr/bin/env bash
# Claude Code hook: maps a hook event (JSON on stdin) to the pane's agent state.
# Does nothing outside tmux. Installed by scripts/install-claude-hooks.sh.
[ -n "${TMUX_PANE:-}" ] || exit 0
command -v jq >/dev/null || exit 0

input="$(cat)"
# Unit separator, not tab: read would merge empty tab-separated fields.
fields="$(printf '%s' "$input" | jq -r '[.hook_event_name, .session_id, .tool_name, .notification_type] | map(. // "") | join("\u001f")' 2>/dev/null)" || exit 0
IFS=$'\037' read -r event sid tool ntype <<EOF
$fields
EOF

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
  Stop) state=idle ;;
  StopFailure) state=error ;;
  SessionEnd) state="" ;;
  *) exit 0 ;;
esac

exec "$(dirname "$0")/../scripts/state.sh" set "$TMUX_PANE" "$state" "$sid"
