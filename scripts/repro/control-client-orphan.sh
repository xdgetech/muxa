#!/usr/bin/env bash
# Fact: a control client whose input and output end together while tmux still
# has output for it (a busy pane, a holder that stopped reading) stays attached
# after stdin EOF, and the server keeps its pipes. Killing the client process
# ends it. With an idle pane, stdin EOF alone ends it.
set -euo pipefail
MUXA_SOCKET="muxa-repro-$$"
. "$(dirname "$0")/../lib/tmux.bash"
command -v python3 >/dev/null || skip "needs python3"
command -v lsof >/dev/null || skip "needs lsof"
trap 'mt_down' EXIT

# One process holds both ends of the client's pipes, as a bridge does. It reads
# READ bytes, stops reading for PAUSE s, then dies by SIGKILL. Prints the pid.
holder() {
  python3 -c '
import os, subprocess, sys, time
p = subprocess.Popen(sys.argv[3:], stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                     stderr=subprocess.DEVNULL)
print(p.pid, flush=True)
p.stdin.write(b"display-message -p ready\n"); p.stdin.flush()
p.stdout.read(int(sys.argv[1]))
time.sleep(float(sys.argv[2]))
os.kill(os.getpid(), 9)
' "$1" "$2" tmux -L "$MUXA_SOCKET" -f /dev/null -C attach-session -t "$3" -f ignore-size \
    2>/dev/null | head -1 || true
}
server_pipes() { lsof -p "$(mt display-message -p '#{pid}')" 2>/dev/null | grep -c PIPE || true; }
alive() { kill -0 "$1" 2>/dev/null; }

busy="$(mt new-session -d -P -F '#{session_id}' -x 200 -y 50 'while :; do seq 1 200; done')"
idle="$(mt new-session -d -P -F '#{session_id}' -x 80 -y 24 sh)"
base="$(server_pipes)"

# 1. Idle pane: stdin EOF ends the client.
client="$(holder 64 0.5 "$idle")"
sleep 3
alive "$client" && fail "with an idle pane, the client stayed after its holder died"
pass "idle pane: the client exits when its holder dies"

# 2. Busy pane, holder stopped reading: the client stays, with its pipes.
client="$(holder 4096 1 "$busy")"
sleep 5
alive "$client" || skip "the client exited; this tmux flushes or drops output on exit"
[ "$(server_pipes)" -ge $((base + 2)) ] || fail "the server does not hold the orphan's pipes"
pass "busy pane: the client stays attached after stdin EOF, and the server keeps its stdin and stdout"

# 3. Killing the client process ends it, and the server lets go of its pipes.
kill -KILL "$client"
wait_for "[ \"\$(server_pipes)\" -le $base ]" || fail "the server kept the pipes of a killed client"
pass "killing the client process frees it at once"
