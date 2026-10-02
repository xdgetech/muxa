# shellcheck shell=bash
# Shared helpers. Every script sources this first. Targets bash 3.2 (macOS).

AGENTS_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
AGENTS_SCRIPTS="$AGENTS_ROOT/scripts"
# shellcheck disable=SC2034  # used by scripts that source this file
TAB=$'\t'

# Higher rank wins when a session rolls up the state of its panes.
# shellcheck disable=SC2034  # used by scripts that source this file
AGENTS_AWK_RANK='function rank(s) { return s=="waiting" ? 5 : s=="error" ? 4 : s=="running" ? 3 : s=="idle" ? 2 : s=="done" ? 1 : 0 }'

# tmux, on a named test server when TMUX_AGENTS_SOCKET is set.
t() {
  if [ -n "${TMUX_AGENTS_SOCKET:-}" ]; then
    tmux -L "$TMUX_AGENTS_SOCKET" "$@"
  else
    tmux "$@"
  fi
}

# Global option value, or the default when it is unset or empty.
opt() {
  local v
  v="$(t show-options -gqv "$1" 2>/dev/null)"
  printf '%s' "${v:-$2}"
}

# Short path on purpose: a Unix socket path must stay under 104 bytes on macOS.
agents_run_dir() {
  local d="${TMPDIR:-/tmp}"
  d="${d%/}/tmux-agents-$(id -u)"
  mkdir -p "$d" && printf '%s' "$d"
}

agents_log_file() { printf '%s/agents.log' "$(agents_run_dir)"; }

log() { printf '%s %s\n' "$(date '+%F %T')" "$*" >>"$(agents_log_file)" 2>/dev/null || true; }

# Tell the user on the tmux client, and keep it in the log.
say() {
  log "$*"
  t display-message "agents: $*" 2>/dev/null || printf 'agents: %s\n' "$*" >&2
}

die() { say "$*"; exit 1; }

# A session name tmux accepts: no '.', ':', '/' or spaces.
session_name_for() { printf '%s' "$1" | tr '.: /' '----'; }

# Session-level option as seen from any target (pane, window or session).
target_opt() { t display-message -p -t "$1" "#{$2}" 2>/dev/null; }

# The main checkout of the repo that contains <dir>, also when <dir> is a worktree.
main_repo() {
  local common
  common="$(git -C "$1" rev-parse --path-format=absolute --git-common-dir 2>/dev/null)" || return 1
  (cd "$common/.." && pwd)
}

# Branch new work starts from: origin's default branch, else the main checkout's branch.
default_base() {
  git -C "$1" symbolic-ref -q --short refs/remotes/origin/HEAD 2>/dev/null \
    || git -C "$1" rev-parse --abbrev-ref HEAD
}

# Refresh the status bar and every sidebar, without blocking the caller.
notify_change() {
  "$AGENTS_SCRIPTS/status.sh" 2>/dev/null
  "$AGENTS_SCRIPTS/refresh.sh" </dev/null >/dev/null 2>&1 &
}
