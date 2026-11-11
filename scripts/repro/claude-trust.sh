#!/usr/bin/env bash
# Fact (docs/findings/claude-code-trust.md): interactive Claude Code shows its
# trust dialog, with "Yes, I trust this folder" on one line, in a folder that
# neither it nor a parent was trusted in, and no dialog in a subfolder of a
# trusted folder. Uses a throwaway CLAUDE_CONFIG_DIR, never ~/.claude.json.
# Runs only with MUXA_REPRO_CLAUDE=1: CI has no Claude Code.
set -euo pipefail
MUXA_SOCKET="muxa-repro-$$"
. "$(dirname "$0")/../lib/tmux.bash"

if [ "${MUXA_REPRO_CLAUDE:-}" != 1 ] || ! command -v claude >/dev/null; then
  printf 'SKIP  set MUXA_REPRO_CLAUDE=1, with claude on PATH\n'
  exit 0
fi
base="$(cd "$(mktemp -d "${TMPDIR:-/tmp}/muxa-trust.XXXXXX")" && pwd -P)"
trap 'mt_down; rm -rf "$base"' EXIT
mkdir -p "$base/cfg" "$base/trusted/assistant/run-1" "$base/other/run-2"
printf '{"hasCompletedOnboarding":true,"projects":{"%s":{"hasTrustDialogAccepted":true}}}\n' \
  "$base/trusted" >"$base/cfg/.claude.json"
printf 'claude %s\n' "$(claude --version)"

# The screen of claude started in $1, after it settled.
screen_in() {
  local pane
  pane="$(mt new-session -d -P -F '#{pane_id}' -x 80 -y 24 -c "$1" \
    "env CLAUDE_CONFIG_DIR='$base/cfg' claude")"
  sleep 8
  mt capture-pane -p -J -t "$pane"
  mt kill-session -t "$pane"
}

screen_in "$base/other/run-2" | grep -q 'Yes, I trust this folder' ||
  fail "no trust dialog in an untrusted folder"
pass "an untrusted folder shows the trust dialog"
if screen_in "$base/trusted/assistant/run-1" | grep -q 'trust this folder'; then
  fail "a subfolder of a trusted folder shows the trust dialog"
fi
pass "a subfolder of a trusted folder shows no dialog"
