#!/usr/bin/env bash
# An isolated tmux server for development and tests: two sessions, each with
# an agent pane (a fake agent) and a shell pane, tagged with @muxa_role.
#
# Usage: tmux-sandbox.sh up | down | attach | status
# Env:   MUXA_SOCKET          server name for tmux -L (default muxa-sandbox)
#        MUXA_SANDBOX_PLUGIN  optional tmux plugin entry file to run inside the
#                             server before the sessions exist (e.g. a sidebar plugin)
set -euo pipefail
. "$(dirname "$0")/lib/tmux.bash"

SESSIONS=(demo-a demo-b)
SIZE_X=200
SIZE_Y=50

up() {
  if mt_running; then
    echo "sandbox '$MUXA_SOCKET' is already running. Run: $0 down" >&2
    exit 1
  fi
  # A boot session keeps the server alive while a plugin installs its hooks.
  mt new-session -d -s _boot -x "$SIZE_X" -y "$SIZE_Y" sh
  mt set-option -g default-shell /bin/sh
  if [ -n "${MUXA_SANDBOX_PLUGIN:-}" ]; then
    # run-shell sets TMUX for the plugin, so its own tmux calls reach this server.
    mt run-shell "$MUXA_SANDBOX_PLUGIN"
  fi
  local name agent shell
  for name in "${SESSIONS[@]}"; do
    agent="$(mt new-session -d -P -F '#{pane_id}' -s "$name" -x "$SIZE_X" -y "$SIZE_Y" \
      "sh '$MUXA_LIB/fake-agent.sh'; exec sh")"
    mt set-option -p -t "$agent" @muxa_role agent
    shell="$(mt split-window -h -d -P -F '#{pane_id}' -t "$agent" sh)"
    mt set-option -p -t "$shell" @muxa_role shell
  done
  mt kill-session -t _boot
  status
  echo
  echo "attach:   MUXA_SOCKET=$MUXA_SOCKET $0 attach   (or: tmux -L $MUXA_SOCKET attach)"
  echo "teardown: MUXA_SOCKET=$MUXA_SOCKET $0 down"
}

down() {
  mt kill-server 2>/dev/null || true
}

status() {
  mt_running || { echo "sandbox '$MUXA_SOCKET' is not running" >&2; exit 1; }
  echo "sandbox '$MUXA_SOCKET':"
  mt list-panes -a -F '  #{session_name} #{pane_id} role=#{?@muxa_role,#{@muxa_role},-} #{pane_width}x#{pane_height} #{pane_current_command}'
}

case "${1:-}" in
  up) up ;;
  down) down ;;
  attach) exec tmux -L "$MUXA_SOCKET" attach-session ;;
  status) status ;;
  *) echo "usage: $0 up | down | attach | status" >&2; exit 2 ;;
esac
