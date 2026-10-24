#!/usr/bin/env bash
# Proves on the developer's real tmux server that muxa changes nothing:
# records every layout, lists sessions and watches one pane read-only through
# the bridge, records again, and diffs. For a human to run; agents must not.
#
# Usage: MUXA_LIVE=1 scripts/live-check.sh SESSION [SECONDS]
# Env:   MUXA_SOCKET  check an isolated -L server instead (for testing this script)
#        MUXA_BRIDGE  bridge command (default: ~/.local/bin/muxa-bridge)
#        MUXA_CLI     client binary  (default: ~/.local/bin/muxa-cli)
set -euo pipefail

if [ "${MUXA_LIVE:-}" != 1 ]; then
  echo "This touches your real tmux server (read-only). Run it yourself: MUXA_LIVE=1 $0 SESSION" >&2
  exit 2
fi
session="${1:?usage: MUXA_LIVE=1 $0 SESSION [SECONDS]}"
seconds="${2:-5}"
bridge="${MUXA_BRIDGE:-$HOME/.local/bin/muxa-bridge}"
record_args=()
if [ -n "${MUXA_SOCKET:-}" ]; then
  bridge="$bridge --socket $MUXA_SOCKET"
  record_args=(-L "$MUXA_SOCKET")
fi
cli="${MUXA_CLI:-$HOME/.local/bin/muxa-cli}"
here="$(cd "$(dirname "$0")" && pwd)"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

"$here/layout-snapshot.sh" record ${record_args[@]+"${record_args[@]}"} >"$work/before"
echo "== sessions as muxa sees them"
"$cli" --bridge "$bridge" list
echo "== watching $session read-only for ${seconds}s"
"$cli" --bridge "$bridge" watch "$session" --read-only --seconds "$seconds" </dev/null >/dev/null
sleep 1
"$here/layout-snapshot.sh" record ${record_args[@]+"${record_args[@]}"} >"$work/after"
echo "== layout diff ($(wc -l <"$work/before" | tr -d ' ') panes and buffers recorded)"
"$here/layout-snapshot.sh" diff "$work/before" "$work/after"
