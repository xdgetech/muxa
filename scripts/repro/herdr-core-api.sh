#!/usr/bin/env bash
# Fact (docs/findings/session-backends.md): herdr-core's framed API gives a
# session backend what ADR 0067 needs: run, attach with a first paint, input,
# resize, OSC 7501 status, events and kill. herdr-core has no isolated
# server mode, so this uses the user's running core: it adds one labelled
# terminal and removes it. Humans only, with MUXA_REPRO_HERDR=1; CI skips.
set -euo pipefail

if [ "${MUXA_REPRO_HERDR:-}" != 1 ] || ! command -v hc >/dev/null; then
  printf 'SKIP  set MUXA_REPRO_HERDR=1, with hc on PATH\n'
  exit 0
fi
dir="$(mktemp -d "${TMPDIR:-/tmp}/muxa-herdr.XXXXXX")"
trap 'rm -rf "$dir"' EXIT
python3 -I "$(dirname "$0")/herdr_core_api.py" "$dir"
