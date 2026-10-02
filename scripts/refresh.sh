#!/usr/bin/env bash
# Tells every sidebar's fzf to reload its tree, through its Unix socket.
set -u
. "$(dirname "$0")/lib.sh"

t list-panes -a -F "#{pane_id}${TAB}#{@agents_sock}" 2>/dev/null |
  while IFS="$TAB" read -r pane sock; do
    [ -n "$sock" ] && [ -S "$sock" ] || continue
    curl -s -m 1 --unix-socket "$sock" -XPOST http://localhost \
      -d "reload('$AGENTS_SCRIPTS/tree.sh' $pane)" >/dev/null 2>&1 &
  done
wait
