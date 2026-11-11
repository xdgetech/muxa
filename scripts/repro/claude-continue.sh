#!/usr/bin/env bash
# Fact (docs/findings/claude-code-resume.md): what interactive Claude Code
# does with --continue and --resume <id> in a folder that has no earlier
# conversation. The root Ask session (ADR 0054) would start with one of them.
# Uses a throwaway CLAUDE_CONFIG_DIR, never ~/.claude.json, and types nothing,
# so no turn runs. Runs only with MUXA_REPRO_CLAUDE=1: CI has no Claude Code.
set -euo pipefail
MUXA_SOCKET="muxa-repro-$$"
. "$(dirname "$0")/../lib/tmux.bash"

if [ "${MUXA_REPRO_CLAUDE:-}" != 1 ] || ! command -v claude >/dev/null; then
  printf 'SKIP  set MUXA_REPRO_CLAUDE=1, with claude on PATH\n'
  exit 0
fi
base="$(cd "$(mktemp -d "${TMPDIR:-/tmp}/muxa-continue.XXXXXX")" && pwd -P)"
trap 'mt_down; rm -rf "$base"' EXIT
mkdir -p "$base/cfg" "$base/root"
printf '{"hasCompletedOnboarding":true,"projects":{"%s":{"hasTrustDialogAccepted":true}}}\n' \
  "$base/root" >"$base/cfg/.claude.json"
printf 'claude %s\n' "$(claude --version)"
# A session keeps the server up; remain-on-exit keeps an exited claude's screen.
mt new-session -d -s hold \; set-option -g remain-on-exit on

# "<dead> TAB <screen>" of `claude $*` in the trusted, empty root, after it settled.
run_in_root() {
  local pane
  pane="$(mt new-session -d -P -F '#{pane_id}' -x 100 -y 24 -c "$base/root" \
    "env CLAUDE_CONFIG_DIR='$base/cfg' claude $*")"
  sleep 8
  printf '%s\t' "$(mt display-message -p -t "$pane" '#{pane_dead}')"
  mt capture-pane -p -J -S - -t "$pane" | tr -s '\n ' ' '
  mt kill-session -t "$pane"
}

out="$(run_in_root --continue)"
case "$out" in
  1*"No conversation found to continue"*) pass "--continue without a conversation exits: No conversation found to continue" ;;
  0*) fail "--continue without a conversation stays open; screen: ${out#*	}" ;;
  *) fail "--continue without a conversation exits with another message: ${out#*	}" ;;
esac

out="$(run_in_root --resume 00000000-0000-4000-8000-000000000000)"
case "$out" in
  1*) pass "--resume with an unknown id exits: $(printf '%s' "${out#*	}" | tr -s ' ' | cut -c 1-120)" ;;
  *) fail "--resume with an unknown id stays open; screen: ${out#*	}" ;;
esac
