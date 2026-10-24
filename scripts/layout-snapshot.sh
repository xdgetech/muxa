#!/usr/bin/env bash
# Records every window's layout and every pane's position, size and process,
# so a before/after diff proves a muxa client changed nothing on the desktop.
# Read-only: it only runs list-panes and list-buffers.
#
# Usage: layout-snapshot.sh record [-L socket] > before.txt
#        layout-snapshot.sh diff before.txt after.txt   (exit 1 when they differ)
set -euo pipefail

# pane_pid catches a restarted or duplicated process. The foreground command
# is left out: it changes whenever an agent runs a tool.
PANE_FORMAT='pane #{session_id} #{window_id} #{window_layout} active=#{window_active} #{pane_id} active=#{pane_active} at=#{pane_left},#{pane_top} #{pane_width}x#{pane_height} pid=#{pane_pid}'

record() {
  local args=()
  if [ "${1:-}" = -L ]; then
    [ -n "${2:-}" ] || { echo "usage: $0 record [-L socket]" >&2; exit 2; }
    args=(-L "$2")
  fi
  # ${args[@]+...}: bash 3.2 (macOS) treats an empty array as unset under set -u.
  tmux ${args[@]+"${args[@]}"} list-panes -a -F "$PANE_FORMAT" | LC_ALL=C sort
  tmux ${args[@]+"${args[@]}"} list-buffers -F 'buffer #{buffer_name}' 2>/dev/null | LC_ALL=C sort || true
}

case "${1:-}" in
  record) shift; record "$@" ;;
  diff)
    [ $# -eq 3 ] || { echo "usage: $0 diff before.txt after.txt" >&2; exit 2; }
    diff -u "$2" "$3" && echo "no layout change"
    ;;
  *) echo "usage: $0 record [-L socket] | diff before after" >&2; exit 2 ;;
esac
