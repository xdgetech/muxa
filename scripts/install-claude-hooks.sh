#!/usr/bin/env bash
# Adds (or with --uninstall removes) the tmux-agents hook in Claude Code's user
# settings. Writes a timestamped backup first. Other hooks are left as they are.
# Usage: install-claude-hooks.sh [--uninstall] [settings.json]
set -eu
. "$(dirname "$0")/lib.sh"

mode=install
[ "${1:-}" = --uninstall ] && { mode=uninstall; shift; }
file="${1:-$HOME/.claude/settings.json}"
hook="$AGENTS_ROOT/hooks/claude-hook.sh"
events='["SessionStart","UserPromptSubmit","PreToolUse","PostToolUse","PostToolUseFailure","PermissionRequest","Elicitation","Notification","Stop","StopFailure","SessionEnd"]'

command -v jq >/dev/null || { echo "jq is required" >&2; exit 1; }
[ -f "$file" ] || { mkdir -p "$(dirname "$file")"; echo '{}' >"$file"; }
jq empty "$file" || { echo "$file is not valid JSON; not changed" >&2; exit 1; }

backup="$file.bak.$(date +%Y%m%d%H%M%S)"
cp "$file" "$backup"

# Drop our entry from every event first, so install is idempotent.
# shellcheck disable=SC2016  # jq variables, not shell ones
filter='
  def without_ours: map(.hooks |= map(select(.command != $cmd))) | map(select((.hooks | length) > 0));
  .hooks = ((.hooks // {}) | with_entries(.value |= without_ours) | with_entries(select((.value | length) > 0)))
'
if [ "$mode" = install ]; then
  # shellcheck disable=SC2016
  filter="$filter"' | reduce $events[] as $e (.; .hooks[$e] = ((.hooks[$e] // []) + [{"hooks": [{"type": "command", "command": $cmd, "timeout": 5}]}]))'
fi

tmp="$(mktemp "$file.XXXXXX")"
jq --arg cmd "$hook" --argjson events "$events" "$filter" "$file" >"$tmp"
mv "$tmp" "$file"
echo "$mode: $hook in $file (backup: $backup)"
