#!/usr/bin/env bash
# Codex `notify` program: marks the pane done when a turn completes.
# Codex sends only agent-turn-complete, so a Codex pane never shows running or waiting.
# Config: notify = ["/path/to/tmux-agents/hooks/codex-notify.sh"] in ~/.codex/config.toml
[ -n "${TMUX_PANE:-}" ] || exit 0
command -v jq >/dev/null || exit 0

for json; do :; done   # the payload is the last argument
type="$(printf '%s' "${json:-}" | jq -r '.type // empty' 2>/dev/null)"
[ "$type" = agent-turn-complete ] || exit 0
exec "$(dirname "$0")/../scripts/state.sh" set "$TMUX_PANE" "done"
