#!/usr/bin/env bash
# Stops what a task left running in its worktree: processes whose cwd is in it,
# also orphans, and Docker Compose projects started from it. Volumes are kept.
# Usage: stop-worktree.sh <worktree>
set -u
. "$(dirname "$0")/lib.sh"

# Seconds a process gets to exit after SIGTERM, before SIGKILL.
GRACE=5

dir="$(cd "${1:-}" 2>/dev/null && pwd -P)" || die "usage: stop-worktree.sh <worktree>"
# Compose records the path it was started from, which may go through a symlink.
given="${1%/}"
case "$dir" in /|"$(cd ~ && pwd -P)") die "refusing to stop processes in $dir" ;; esac

# This script and its callers can run with their cwd in the worktree.
ancestors() {
  local p=$$
  while [ -n "$p" ] && [ "$p" -gt 1 ]; do
    echo "$p"
    p="$(ps -o ppid= -p "$p" 2>/dev/null | tr -d ' ')"
  done
}

pids_in() {
  local skip
  skip="$(ancestors | tr '\n' ' ')"
  lsof -nP -a -d cwd -Fpn 2>/dev/null |
    awk -v d="$dir" -v skip=" $skip" '
      /^p/ { pid = substr($0, 2) }
      /^n/ { n = substr($0, 2); if ((n == d || index(n, d "/") == 1) && index(skip, " " pid " ") == 0) print pid }'
}

stop_processes() {
  local pids i
  pids="$(pids_in)"
  [ -n "$pids" ] || return 0
  log "stopping in $dir: ${pids//$'\n'/ }"
  # shellcheck disable=SC2086  # one pid per word
  kill -TERM $pids 2>/dev/null
  for i in $(seq $((GRACE * 10))); do
    pids="$(pids_in)"
    [ -n "$pids" ] || return 0
    sleep 0.1
  done
  log "killing in $dir: ${pids//$'\n'/ }"
  # shellcheck disable=SC2086
  kill -KILL $pids 2>/dev/null
  return 0
}

compose_projects() {
  docker ps -a --format '{{.Label "com.docker.compose.project"}}	{{.Label "com.docker.compose.project.working_dir"}}' 2>/dev/null |
    awk -F'\t' -v d="$dir" -v g="$given" '
      function under(x, p) { return x == p || index(x, p "/") == 1 }
      $1 != "" && (under($2, d) || under($2, g)) { print $1 }' | sort -u
}

stop_compose() {
  command -v docker >/dev/null || return 0
  local p
  for p in $(compose_projects); do
    log "docker compose down: $p"
    docker compose -p "$p" down >>"$(agents_log_file)" 2>&1 \
      || say "could not stop the compose project $p. Details: $(agents_log_file)"
  done
  return 0
}

stop_processes
stop_compose
