#!/bin/sh
# Stops a throwaway test server for good. With an owner pid it first waits for
# that process (a test run) to exit, so a run killed before its teardown still
# leaves nothing behind.
# Usage: tmux-reaper.sh <server pid> <socket path> [owner pid]
set -u
server=$1 socket=$2 owner=${3:-}
name=${socket##*/}

# By its arguments, so a reused pid is never taken for the server.
alive() {
  case $(ps -o args= -p "$server" 2>/dev/null) in
    *" -L $name "*) return 0 ;;
  esac
  return 1
}

if [ -n "$owner" ]; then
  while alive && kill -0 "$owner" 2>/dev/null; do sleep 1; done
  if alive; then kill -TERM "$server" 2>/dev/null; fi
fi
# A server waiting for a control client that never leaves refuses new
# connections, so kill-server cannot reach it. By pid it still goes, and its
# clients exit with it.
for _ in 1 2 3 4 5 6 7 8 9 10; do
  alive || break
  sleep 0.1
done
if alive; then kill -KILL "$server" 2>/dev/null; fi
# tmux 3.7 leaves its socket file behind, even after kill-server.
rm -f "$socket"
