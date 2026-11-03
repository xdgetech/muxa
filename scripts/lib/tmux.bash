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
  [ -z "${MT_CONTROL_DIR:-}" ] || rm -rf "$MT_CONTROL_DIR"
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
# $3: optional client flags. Its output goes to $MT_CONTROL_LOG (default a
# temporary file that mt_down removes).
# Usage: mt_control_open 3 "$session" ignore-size; echo 'refresh-client -C 90x30' >&3
mt_control_open() {
  local fifo log="${MT_CONTROL_LOG:-}"
  if [ -z "$log" ]; then
    [ -n "${MT_CONTROL_DIR:-}" ] || MT_CONTROL_DIR="$(mktemp -d "${TMPDIR:-/tmp}/muxa-ctl-logs.XXXXXX")"
    log="$MT_CONTROL_DIR/$1.log"
  fi
  eval "MT_CONTROL_LOG_$1=\"\$log\""
  fifo="$(mktemp -u "${TMPDIR:-/tmp}/muxa-ctl.XXXXXX")"
  mkfifo "$fifo"
  if [ -n "${3:-}" ]; then
    mt -C attach-session -t "$2" -f "$3" <"$fifo" >"$log" 2>&1 &
  else
    mt -C attach-session -t "$2" <"$fifo" >"$log" 2>&1 &
  fi
  eval "MT_CONTROL_PID_$1=$!"
  eval "exec $1>\"\$fifo\""
  rm -f "$fifo"
}

# Closes the control client on file descriptor $1 and waits until its process
# exits. A client whose session was killed leaves list-clients at once, but
# tmux handles its loss only after the process exits.
mt_control_close() {
  local pid
  eval "pid=\"\${MT_CONTROL_PID_$1:-}\""
  eval "exec $1>&-"
  [ -z "$pid" ] || wait "$pid" 2>/dev/null || true
}

# Waits until the control client on file descriptor $1 has run every command
# written to it so far. A client runs its commands in order, so the output of
# a marker written last proves the rest ran. Unlike a sleep, this holds on a
# loaded machine; display-message changes nothing on the server.
MT_SYNC=0
mt_control_sync() {
  local log
  eval "log=\"\${MT_CONTROL_LOG_$1:-}\""
  [ -n "$log" ] || { echo "mt_control_sync: fd $1 was not opened by mt_control_open" >&2; return 1; }
  MT_SYNC=$((MT_SYNC + 1))
  echo "display-message -p muxa-sync-$MT_SYNC" >&"$1"
  wait_for "grep -qx 'muxa-sync-$MT_SYNC' '$log'"
}

# A real (non-control) client on a pty, like a desktop terminal.
# Usage: mt_desktop <session id> <cols> <rows> <seconds> [<type "x" after seconds>] &
mt_desktop() {
  python3 -I "$MUXA_LIB/pty-client.py" "$MUXA_SOCKET" "$@"
}

# Types "x" on the desktop client fed by file descriptor $1 (mt_desktop with
# "stdin"), and waits until its pane, a shell, echoes it: tmux handled the key.
mt_desktop_type() {
  local pane before
  pane="$(mt list-clients -f '#{!=:#{client_control_mode},1}' -F '#{pane_id}' | head -1)"
  before="$(mt_typed "$pane")"
  echo x >&"$1"
  wait_for "[ \"\$(mt_typed '$pane')\" -gt $before ]"
}
mt_typed() { mt capture-pane -p -t "$1" | tr -cd x | wc -c | tr -d ' '; }

window_size() { mt display-message -p -t "$1" '#{window_width}x#{window_height}'; }

# Waits up to $MT_WAIT seconds (default 20) for a shell condition. The bound
# is generous because a loaded machine runs tmux slowly; a met condition
# returns at once.
wait_for() {
  local end=$((SECONDS + ${MT_WAIT:-20}))
  while [ "$SECONDS" -lt "$end" ]; do
    eval "$1" && return 0
    sleep 0.1
  done
  echo "timed out waiting for: $1" >&2
  return 1
}

pass() { printf 'PASS  %s\n' "$*"; }
fail() { printf 'FAIL  %s\n' "$*"; exit 1; }
skip() { printf 'SKIP  %s\n' "$*"; exit 0; }
