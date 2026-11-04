# shellcheck shell=bash
# Shared helpers. Every script sources this first. Targets bash 3.2 (macOS).

AGENTS_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
AGENTS_SCRIPTS="$AGENTS_ROOT/scripts"
# shellcheck disable=SC2034  # used by scripts that source this file
TAB=$'\t'
# A tmux format, expanded for the AI pane. @agent_name is set by layout.sh.
# shellcheck disable=SC2034
AGENTS_LAUNCH_CMD="claude --name '#{@agent_name}'"

# Agent states for awk. Higher rank wins when a session rolls up the state of its
# panes. "done" is a finished turn nobody has looked at yet, so it beats idle.
# sicon() needs the icon variables that awk_states passes.
# shellcheck disable=SC2034  # used by scripts that source this file
AGENTS_AWK_STATE='
  function rank(s) { return s=="waiting" ? 5 : s=="error" ? 4 : s=="running" ? 3 : s=="done" ? 2 : s=="idle" ? 1 : 0 }
  function sicon(r) { return r==5 ? IW : r==4 ? IE : r==3 ? IR : r==2 ? ID : r==1 ? II : "" }
  function sword(r) { return r==5 ? "needs you" : r==4 ? "error" : r==3 ? "working" : r==2 ? "done" : r==1 ? "idle" : "no agent" }
'

# The tree's group of a session (ADR 0037): its repo; else a muxa Task's group,
# "}r" for a Routine run (its folder is <root>/routines/...) or "}t"; else "~",
# the manual sessions. In byte order the groups follow the repos, sessions last.
# shellcheck disable=SC2034  # used by scripts that source this file
AGENTS_AWK_GROUP='
  function group(repo, task, path) {
    if (repo != "") return repo
    if (task == "") return "~"
    return (path ~ /\/routines\/[^\/]+$/ ? "}r" : "}t")
  }
  function glabel(g) { return g == "~" ? "sessions" : g == "}r" ? "routines" : g == "}t" ? "tasks" : g }
'

# A -f filter for list-sessions, list-windows -a and list-panes -a. It skips
# muxa's device sessions (session option @muxa_device): each holds one window
# of a session while a phone or tablet shows it, so lists would show it twice.
# shellcheck disable=SC2034  # used by scripts that source this file
AGENTS_VISIBLE='#{==:#{@muxa_device},}'

# Keys that jump to the 1st, 2nd, ... session of the tree, as tree.sh labels them.
# Letters work only after prefix + j: the tree uses some of them for its own keys.
# shellcheck disable=SC2034  # used by scripts that source this file
AGENTS_JUMP_KEYS=123456789abcdefghijklmnopqrstuvwxyz

# awk with the state icons from the @agents_icon_* options, read in one tmux call.
# LC_ALL=C: byte order for sorting, so the "~" group sorts after every repo.
awk_states() {
  local w e r i d
  IFS=$'\037' read -r w e r i d <<EOF
$(t display-message -p "#{@agents_icon_waiting}"$'\037'"#{@agents_icon_error}"$'\037'"#{@agents_icon_running}"$'\037'"#{@agents_icon_idle}"$'\037'"#{@agents_icon_done}" 2>/dev/null)
EOF
  LC_ALL=C awk -v IW="${w:-🙋}" -v IE="${e:-❌}" -v IR="${r:-⚡}" -v II="${i:-💤}" -v ID="${d:-🏁}" "$@"
}

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

# The rows a sidebar's fzf shows, as last written by refresh.sh.
sidebar_rows_file() { printf '%s/sb-%s.rows' "$(agents_run_dir)" "${1#%}"; }

agents_log_file() { printf '%s/agents.log' "$(agents_run_dir)"; }

log() { printf '%s %s\n' "$(date '+%F %T')" "$*" >>"$(agents_log_file)" 2>/dev/null || true; }

# Tell the user on the tmux client, and keep it in the log. A caller outside
# tmux (muxa) gets no client message, so it reads the reason on stderr.
say() {
  log "$*"
  t display-message "agents: $*" 2>/dev/null && [ -n "${TMUX:-}" ] && return 0
  printf 'agents: %s\n' "$*" >&2
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

# Where a session works: its worktree, else its active pane's directory.
session_dir() { t display-message -p -t "=$1:" '#{?@agent_worktree,#{@agent_worktree},#{pane_current_path}}' 2>/dev/null; }

# "<branch> TAB <checkout top dir>" for <dir>, read live. Detached HEAD gives the short hash.
git_where() {
  local out branch
  out="$(git -C "$1" rev-parse --abbrev-ref HEAD --show-toplevel 2>/dev/null)" || return 1
  branch="${out%%$'\n'*}"
  [ "$branch" = HEAD ] && branch="$(git -C "$1" rev-parse -q --short HEAD)"
  printf '%s\t%s\n' "$branch" "${out#*$'\n'}"
}

# Branch new work starts from: origin's default branch, else the main checkout's branch.
default_base() {
  git -C "$1" symbolic-ref -q --short refs/remotes/origin/HEAD 2>/dev/null \
    || git -C "$1" rev-parse --abbrev-ref HEAD
}

# Windows someone is looking at: each attached client's current window, when its
# terminal has focus. With focus-events off tmux cannot tell, so every client counts.
# A control-mode client (tmux -C) is a program, not a person, so it never counts.
watched_windows() {
  local need=focused
  [ "$(t show-options -gv focus-events 2>/dev/null)" = on ] || need=""
  t list-clients -F "#{window_id}${TAB}#{client_flags}" 2>/dev/null |
    awk -F'\t' -v need="$need" '$2 !~ /(^|,)control-mode(,|$)/ && (need == "" || $2 ~ /(^|,)focused(,|$)/) { print $1 }'
}

# A Claude background job runs outside tmux, so its hooks get no TMUX_PANE.
# Its pane is the one agent pane in the job's directory. None or several: no pane.
agent_pane_in() {
  [ -n "$1" ] || return 0
  t list-panes -a -f "$AGENTS_VISIBLE" -F "#{pane_id}${TAB}#{pane_current_path}${TAB}#{@agent_session_id}" 2>/dev/null |
    awk -F'\t' -v d="$1" '$2 == d && $3 != "" { n++; p = $1 } END { if (n == 1) print p }'
}

is_watched() {
  local w
  w="$(t display-message -p -t "$1" '#{window_id}' 2>/dev/null)" || return 1
  watched_windows | grep -qxF "$w"
}

# Refresh the status bar and every sidebar, without blocking the caller.
notify_change() {
  "$AGENTS_SCRIPTS/status.sh" 2>/dev/null
  "$AGENTS_SCRIPTS/refresh.sh" </dev/null >/dev/null 2>&1 &
}
