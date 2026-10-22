#!/usr/bin/env bash
# Creates a task: a worktree, a session in it, and the AI pane | terminal pane layout.
# Prints the session name. An existing session for the task is reused.
# Usage: new.sh <repo dir> <branch> [--task ID] [--base REF] [--no-agent] [--no-switch]
set -u
. "$(dirname "$0")/lib.sh"

USAGE="usage: new.sh <repo dir> <branch> [--task ID] [--base REF] [--no-agent] [--no-switch]"
[ $# -ge 2 ] && [ -n "$1" ] && [ -n "$2" ] || die "$USAGE"
repo_in="$1" branch="$2"
shift 2
task="" base="" agent=1 switch=1
while [ $# -gt 0 ]; do
  case "$1" in
    --task) task="${2:-}"; shift 2 ;;
    --base) base="${2:-}"; shift 2 ;;
    --no-agent) agent=0; shift ;;
    --no-switch) switch=0; shift ;;
    *) die "unknown option $1. $USAGE" ;;
  esac
done

case "$repo_in" in "~"*) repo_in="$HOME${repo_in#\~}" ;; esac
main="$(main_repo "$repo_in")" || die "$repo_in is not a git repository"

# A worktree already checked out on the branch, anywhere except the main checkout.
existing_worktree() {
  git -C "$main" worktree list --porcelain | awk -v b="refs/heads/$branch" -v m="$main" '
    /^worktree / { p = substr($0, 10) }
    /^branch / { if ($2 == b && p != m) { print p; exit } }'
}

# Same location and naming as the orchestrator's wt_path: <repos dir>/.worktrees/<repo>-<branch>.
default_worktree() {
  local found root path
  found="$(existing_worktree)"
  if [ -n "$found" ]; then printf '%s\n' "$found"; return 0; fi

  root="$(opt @agents_worktrees_root "$(dirname "$main")/.worktrees")"
  path="$root/$(basename "$main")-$(printf '%s' "$branch" | tr '/' '-')"
  mkdir -p "$root" || return 1
  git -C "$main" fetch -q origin 2>/dev/null || true
  if git -C "$main" show-ref --verify -q "refs/heads/$branch"; then
    git -C "$main" worktree add -q "$path" "$branch"
  elif git -C "$main" show-ref --verify -q "refs/remotes/origin/$branch"; then
    git -C "$main" worktree add -q --track -b "$branch" "$path" "origin/$branch"
  else
    git -C "$main" worktree add -q -b "$branch" "$path" "${base:-$(default_base "$main")}"
  fi 2>"$err" || return 1
  printf '%s\n' "$path"
}

# Git refuses a second worktree on a branch, and the main checkout is never a task's worktree.
in_main_checkout() {
  [ -z "$(existing_worktree)" ] && [ "$(git -C "$main" branch --show-current 2>/dev/null)" = "$branch" ]
}

# @agents_worktree_cmd lets the orchestrator own worktree creation: it gets
# <main repo> <branch> <base> and prints the worktree path on its last line.
cmd="$(opt @agents_worktree_cmd '')"
err="$(agents_run_dir)/new-$$.err"
trap 'rm -f "$err"' EXIT
if [ -n "$cmd" ]; then
  # shellcheck disable=SC2086  # a command path, optionally with its own flags
  wt="$($cmd "$main" "$branch" "$base" 2>"$err" | tail -n 1)"
else
  in_main_checkout && die "$branch is checked out in the main checkout $main. Switch it to another branch there, then open the task again."
  wt="$(default_worktree)"
fi
[ -s "$err" ] && log "$(cat "$err")"
if [ -z "$wt" ] || [ ! -d "$wt" ]; then
  why="$(sed -n 's/^fatal: //p' "$err" 2>/dev/null | head -n 1)"
  [ -n "$why" ] || why="$(sed '/^$/d' "$err" 2>/dev/null | tail -n 1)"
  die "could not create a worktree for $branch: ${why:-see $(agents_log_file)}"
fi

name="$(session_name_for "${task:-$branch}")"
if t has-session -t "=$name" 2>/dev/null; then
  [ "$switch" = 1 ] && t switch-client -t "=$name" 2>/dev/null
  printf '%s\n' "$name"
  exit 0
fi

win="$(t new-session -d -P -F '#{window_id}' -s "$name" -n "$(opt @agents_window_name main)" -c "$wt" \
  -e "AGENTS_REPO_DIR=$main" -e "AGENTS_WORKTREE=$wt" -e "AGENTS_BRANCH=$branch" -e "AGENTS_TASK=$task")" \
  || die "could not create session $name"

t set-option -t "=$name:" @agent_repo "$(basename "$main")"
t set-option -t "=$name:" @agent_repo_dir "$main"
t set-option -t "=$name:" @agent_worktree "$wt"
t set-option -t "=$name:" @agent_branch "$branch"
[ -n "$task" ] && t set-option -t "=$name:" @agent_task "$task"

if [ "$agent" = 1 ]; then "$AGENTS_SCRIPTS/layout.sh" "$win"; else "$AGENTS_SCRIPTS/layout.sh" "$win" --no-agent; fi
[ "$switch" = 1 ] && t switch-client -t "=$name" 2>/dev/null
notify_change
printf '%s\n' "$name"
