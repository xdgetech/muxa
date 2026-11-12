#!/usr/bin/env bash
# Fact (docs/findings/claude-code-resume.md): what interactive Claude Code
# does with --continue and --resume <id> in a folder that has no earlier
# conversation, and with --resume <id> of a conversation it has. The
# secretary's session (ADR 0054, 0056) starts with --resume <id>.
# Uses a throwaway CLAUDE_CONFIG_DIR, never ~/.claude.json, and types nothing,
# so no turn runs. Runs only with MUXA_REPRO_CLAUDE=1: CI has no Claude Code.
set -euo pipefail
: "${MUXA_SOCKET:=muxa-repro-$$}"
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

# A conversation Claude Code has: a made-up transcript of one turn, in the
# folder Claude Code keeps the root's conversations in.
id=11111111-2222-4333-8444-555555555555
project="$base/cfg/projects/$(printf '%s' "$base/root" | sed 's/[^A-Za-z0-9]/-/g')"
mkdir -p "$project"
{
  printf '{"type":"user","uuid":"u1","parentUuid":null,"isSidechain":false,"sessionId":"%s","cwd":"%s","timestamp":"2026-11-11T00:00:00.000Z","message":{"role":"user","content":"muxa-probe-question"}}\n' "$id" "$base/root"
  printf '{"type":"assistant","uuid":"a1","parentUuid":"u1","isSidechain":false,"sessionId":"%s","cwd":"%s","timestamp":"2026-11-11T00:00:01.000Z","message":{"id":"msg_1","role":"assistant","content":[{"type":"text","text":"muxa-probe-answer"}]}}\n' "$id" "$base/root"
} >"$project/$id.jsonl"
pane="$(mt new-session -d -P -F '#{pane_id}' -x 100 -y 24 -c "$base/root" \
  "env CLAUDE_CONFIG_DIR='$base/cfg' claude --resume $id")"
sleep 8
[ "$(mt display-message -p -t "$pane" '#{pane_dead}')" = 0 ] || fail "--resume with a known id exited"
mt capture-pane -p -J -S - -t "$pane" | grep -q muxa-probe-answer \
  || fail "--resume with a known id does not show the conversation"
pass "--resume with a known id stays open and shows the conversation"
grep -qs "\"sessionId\":\"$id\"" "$base/cfg/sessions/"*.json \
  || fail "the resumed instance's session file has another id: $(cat "$base/cfg/sessions/"*.json 2>/dev/null)"
pass "the resumed instance's sessions/<pid>.json keeps the resumed id: muxa sees the same id again"
