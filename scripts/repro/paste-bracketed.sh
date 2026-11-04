#!/usr/bin/env bash
# Fact: paste-buffer -p wraps the text in bracketed-paste marks only when the
# pane's program turned bracketed paste on, and tmux sends each LF as CR.
# A multi-line reply (ADR 0033, redo and change-course) arrives as one paste.
set -euo pipefail
MUXA_SOCKET="muxa-repro-$$"
. "$(dirname "$0")/../lib/tmux.bash"
trap 'mt_down; rm -rf "$dir"' EXIT

dir="$(mktemp -d)"
# Records the raw bytes it reads. With "on", it first turns bracketed paste on.
cat >"$dir/reader.py" <<'PY'
import os, sys, termios, tty
out, mode = sys.argv[1], sys.argv[2]
fd = sys.stdin.fileno()
tty.setraw(fd)
if mode == "on":
    os.write(1, b"\033[?2004h")
os.write(1, b"ready\r\n")
with open(out, "wb") as f:
    while True:
        b = os.read(fd, 1024)
        if not b:
            break
        f.write(b)
        f.flush()
PY

paste_into() {
  local mode="$1" out="$dir/$1.bin" pane
  pane="$(mt new-session -d -P -F '#{pane_id}' -x 80 -y 20 "python3 '$dir/reader.py' '$out' $mode")"
  wait_until 5 "mt capture-pane -p -t '$pane' | grep -q ready"
  mt set-buffer -b muxa-repro -- $'line one\nline two'
  mt paste-buffer -p -d -b muxa-repro -t "$pane"
  wait_until 5 "[ -s '$out' ]"
  sleep 0.2
  od -An -c "$out" | tr '\n' ' ' | tr -s ' ' | sed 's/^ //'
}

wait_until() {
  local n=$(($1 * 10))
  while [ "$n" -gt 0 ]; do eval "$2" && return 0; sleep 0.1; n=$((n - 1)); done
  fail "timed out: $2"
}

on="$(paste_into on)"
case "$on" in
  *'033 [ 2 0 0 ~ l i n e o n e \r l i n e t w o 033 [ 2 0 1 ~'*) ;;
  *) fail "bracketed paste on: unexpected bytes: $on" ;;
esac
pass "with bracketed paste on, paste-buffer -p sends ESC[200~ text ESC[201~, each LF as CR"

off="$(paste_into off)"
case "$off" in
  *033*) fail "bracketed paste off: marks were sent: $off" ;;
  *'l i n e o n e \r l i n e t w o'*) ;;
  *) fail "bracketed paste off: unexpected bytes: $off" ;;
esac
pass "with bracketed paste off, paste-buffer -p sends no marks, so each line is a separate Enter"
