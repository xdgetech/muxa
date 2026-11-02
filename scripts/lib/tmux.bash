# shellcheck shell=bash
# Helpers for harness scripts. Every tmux call goes to the isolated server
# named by MUXA_SOCKET; nothing here touches the default server.

: "${MUXA_SOCKET:=muxa-sandbox}"
MUXA_LIB="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# tmux on the isolated server, without the user's ~/.tmux.conf.
mt() { tmux -L "$MUXA_SOCKET" -f /dev/null "$@"; }

mt_running() { mt has-session 2>/dev/null; }

# Stops the isolated server and removes the socket file tmux leaves behind.
mt_down() {
  local pid path
  read -r pid path < <(mt display-message -p '#{pid} #{socket_path}' 2>/dev/null) || return 0
  mt kill-server 2>/dev/null || true
  "$MUXA_LIB/tmux-reaper.sh" "$pid" "$path"
}

# A control-mode client on session id $1 that reads command lines from stdin
# and prints everything tmux sends. -f ignore-size: it never sizes a window.
# Returns 0: tmux exits 1 after a command error, and callers check the output.
mt_control() { mt -C attach-session -t "$1" -f ignore-size 2>&1 || true; }

# A control-mode client on session id $2 that stays attached and reads command
# lines written to file descriptor $1 until the caller closes it (exec 3>&-).
# $3: optional client flags. Its output goes to $MT_CONTROL_LOG (default none).
# Usage: mt_control_open 3 "$session" ignore-size; echo 'refresh-client -C 90x30' >&3
mt_control_open() {
  local fifo
  fifo="$(mktemp -u "${TMPDIR:-/tmp}/muxa-ctl.XXXXXX")"
  mkfifo "$fifo"
  if [ -n "${3:-}" ]; then
    mt -C attach-session -t "$2" -f "$3" <"$fifo" >"${MT_CONTROL_LOG:-/dev/null}" 2>&1 &
  else
    mt -C attach-session -t "$2" <"$fifo" >"${MT_CONTROL_LOG:-/dev/null}" 2>&1 &
  fi
  eval "exec $1>\"\$fifo\""
  rm -f "$fifo"
}

# A real (non-control) client on a pty, like a desktop terminal.
# Usage: mt_desktop <session id> <cols> <rows> <seconds> [<type "x" after seconds>] &
mt_desktop() {
  python3 -I "$MUXA_LIB/pty-client.py" "$MUXA_SOCKET" "$@"
}

window_size() { mt display-message -p -t "$1" '#{window_width}x#{window_height}'; }

# Waits up to ~5 s for a shell condition.
wait_for() {
  local _
  for _ in $(seq 50); do
    eval "$1" && return 0
    sleep 0.1
  done
  echo "timed out waiting for: $1" >&2
  return 1
}

pass() { printf 'PASS  %s\n' "$*"; }
fail() { printf 'FAIL  %s\n' "$*"; exit 1; }
skip() { printf 'SKIP  %s\n' "$*"; exit 0; }
