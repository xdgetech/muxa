#!/usr/bin/env bash
# Reloads sidebars through their Unix sockets, but only those whose rows changed.
# An unchanged sidebar is not redrawn, so state changes elsewhere cause no flicker.
# Usage: refresh.sh [sidebar pane]   (default: every sidebar)
set -u
. "$(dirname "$0")/lib.sh"

refresh_one() {
  local pane="$1" sock="$2" rows new
  rows="$(sidebar_rows_file "$pane")"
  new="$(mktemp "$rows.XXXXXX")" || return 1
  "$AGENTS_SCRIPTS/tree.sh" "$pane" >"$new"
  if cmp -s "$new" "$rows"; then rm -f "$new"; return 0; fi
  mv "$new" "$rows"
  curl -s -m 1 --unix-socket "$sock" -XPOST http://localhost -d "reload(cat '$rows')" >/dev/null 2>&1
}

t list-panes -a -F "#{pane_id}${TAB}#{@agents_sock}" 2>/dev/null |
  while IFS="$TAB" read -r pane sock; do
    [ -z "${1:-}" ] || [ "$pane" = "$1" ] || continue
    [ -n "$sock" ] && [ -S "$sock" ] || continue
    refresh_one "$pane" "$sock" &
  done
wait
