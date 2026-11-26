#!/usr/bin/env bash
# Fact (docs/findings/claude-code-input.md): Claude Code reads one long write
# as a paste, so a Return in the same write is a new line, not a send. Runs
# Claude Code on a throwaway tmux server in a throwaway folder; each sent line
# is a short prompt to the model. Humans only, with MUXA_REPRO_CLAUDE=1.
set -euo pipefail

if [ "${MUXA_REPRO_CLAUDE:-}" != 1 ] || ! command -v claude >/dev/null || ! command -v tmux >/dev/null; then
  printf 'SKIP  set MUXA_REPRO_CLAUDE=1, with claude and tmux on PATH\n'
  exit 0
fi
dir="$(mktemp -d "${TMPDIR:-/tmp}/muxa-input.XXXXXX")"
sock="muxa-test-input-$$"
cleanup() { tmux -L "$sock" kill-server 2>/dev/null || true; rm -rf "$dir"; }
trap cleanup EXIT

t() { tmux -L "$sock" "$@"; }
hex() { printf '%s' "$1" | xxd -p -c1 | tr '\n' ' '; }
pad() { printf '%*s' "$1" '' | tr ' ' x; }

t new-session -d -x 120 -y 40 -c "$dir" claude
sleep 8
# The trust dialog of a new folder: pick "Yes, I trust this folder".
if t capture-pane -p | grep -q 'trust this folder'; then
  t send-keys Down; sleep 0.5; t send-keys Enter; sleep 5
fi

# try NAME TEXT GAP: GAP empty writes TEXT and Return at once; else Return
# goes alone after GAP seconds.
try() {
  # shellcheck disable=SC2046 # one word per byte
  if [ -z "$3" ]; then t send-keys -H $(hex "$2") 0d; else t send-keys -H $(hex "$2"); sleep "$3"; t send-keys -H 0d; fi
  sleep 5
  if t capture-pane -p | tail -6 | grep -q 'reply only'; then
    printf '%-24s not sent\n' "$1"
    t send-keys C-u; sleep 0.3; t send-keys C-u; sleep 0.5
  else
    printf '%-24s sent\n' "$1"
  fi
}

try "60 bytes, one write" "reply only: ok $(pad 45)" ""
try "70 bytes, one write" "reply only: ok $(pad 55)" ""
try "300 bytes, Return alone" "reply only: ok $(pad 285)" 0.05
