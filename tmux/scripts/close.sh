#!/usr/bin/env bash
# Closes a task session. Without --keep it also removes the worktree and its
# branch (ADR 0043), but only when nothing can be lost: no uncommitted changes,
# and the branch is merged. --force removes them anyway, after the user said yes.
# --check only prints why removing needs that yes, or nothing.
# In every mode it stops the processes and compose projects left in the worktree.
# --task marks the session's muxa Task done or dropped once the session is gone.
# Usage: close.sh [--keep|--force|--check] [--task done|dropped] <session>
set -u
. "$(dirname "$0")/lib.sh"

keep=0 force=0 check=0 task_status=""
while [ $# -gt 1 ]; do
  case "$1" in
    --keep) keep=1; shift ;;
    --force) force=1; shift ;;
    --check) check=1; shift ;;
    --task) task_status="${2:-}"; shift 2 ;;
    *) break ;;
  esac
done
s="${1:-}"
[ -n "$s" ] || die "usage: close.sh [--keep|--force|--check] [--task done|dropped] <session>"
t has-session -t "=$s" 2>/dev/null || die "no session $s"
task="$(target_opt "=$s:" @muxa_task)"

wt="$(target_opt "=$s:" @agent_worktree)"
main="$(target_opt "=$s:" @agent_repo_dir)"
[ -n "$main" ] || main="$(main_repo "$wt" 2>/dev/null)"
pr_state="$(target_opt "=$s:" @agent_pr_state)"

# Merged = PR merged (@agent_pr_state or gh), or the branch adds nothing to its base.
merged() {
  local br state base
  [ "$pr_state" = merged ] && return 0
  br="$(git -C "$wt" branch --show-current 2>/dev/null)"
  if [ "$(opt @agents_check_gh on)" = on ] && [ -n "$br" ] && command -v gh >/dev/null; then
    state="$(cd "$wt" && gh pr view "$br" --json state -q .state 2>/dev/null)"
    [ "$state" = MERGED ] && return 0
  fi
  base="$(default_base "${main:-$wt}")"
  [ -n "$base" ] && git -C "$wt" merge-base --is-ancestor HEAD "$base" 2>/dev/null
}

# Why removing the worktree could lose work, or nothing.
risk() {
  [ -n "$wt" ] && [ -d "$wt" ] || return 0
  if [ -n "$(git -C "$wt" status --porcelain 2>/dev/null)" ]; then
    echo "it has uncommitted changes"
  elif ! merged; then
    echo "its branch is not merged"
  fi
}

if [ "$check" = 1 ]; then
  risk
  exit 0
fi

remove_wt=0 branch="" real="" base=""
if [ "$keep" = 0 ] && [ -n "$wt" ] && [ -d "$wt" ]; then
  why="$(risk)"
  [ -z "$why" ] || [ "$force" = 1 ] \
    || die "$s: $why. Close it with --force to remove the worktree and branch anyway, or --keep to keep them."
  remove_wt=1
  branch="$(git -C "$wt" branch --show-current 2>/dev/null)"
  real="$(cd "$wt" && pwd -P)"
  base="$(default_base "${main:-$wt}")"
fi

# Without --force the branch goes only when each of its commits is in the base
# branch or on a remote: a merged PR's branch may hold commits made after the merge.
delete_branch() {
  [ -n "$branch" ] && [ -n "$main" ] || return 0
  if [ "$force" = 0 ] && [ -n "$(git -C "$main" rev-list -n 1 "refs/heads/$branch" --not ${base:+"$base"} --remotes 2>/dev/null)" ]; then
    say "kept the branch $branch: it has commits on no remote"
    return 0
  fi
  git -C "$main" branch -D "$branch" >/dev/null 2>>"$(agents_log_file)" \
    || say "could not delete the branch $branch. Details: $(agents_log_file)"
}

# Move attached clients to the most recent other session, so they are not detached.
other="$(t list-sessions -f "$AGENTS_VISIBLE" -F "#{session_last_attached} #{session_name}" |
  awk -v s="$s" '{ n = $0; sub(/^[^ ]* /, "", n); if (n != s) print }' | sort -rn | head -n 1 | cut -d' ' -f2-)"
if [ -n "$other" ]; then
  t list-clients -t "=$s" -F '#{client_name}' 2>/dev/null | while read -r c; do
    t switch-client -c "$c" -t "=$other"
  done
fi

# A muxa device session that holds one of its windows would keep it running.
sid="$(t display-message -p -t "=$s:" '#{session_id}')"
t list-windows -a -F "#{session_id}${TAB}#{window_id}${TAB}#{@muxa_device}" 2>/dev/null |
  awk -F'\t' -v sid="$sid" '
    { s[NR] = $1; w[NR] = $2; d[NR] = $3; if ($1 == sid) own[$2] = 1 }
    END { for (i = 1; i <= NR; i++) if (d[i] != "" && s[i] != sid && (w[i] in own)) print s[i] }' |
  sort -u | while read -r device; do
    t kill-session -t "$device"
  done

t kill-session -t "=$s" || die "could not close $s"
# The session is gone either way, so a muxa failure is reported, not fatal.
if [ -n "$task_status" ] && [ -n "$task" ]; then
  "$AGENTS_SCRIPTS/muxa.sh" close-task "$task" "$task_status" 2>/dev/null \
    || say "closed $s, but muxa could not mark its Task $task_status. Details: $(agents_log_file)"
fi
[ -n "$wt" ] && [ -d "$wt" ] && "$AGENTS_SCRIPTS/stop-worktree.sh" "$wt"
if [ "$remove_wt" = 1 ]; then
  # --force also removes changes and submodules; the user said yes to that.
  rm_flags=()
  [ "$force" = 1 ] && rm_flags=(--force)
  git -C "${main:-$wt}" worktree remove ${rm_flags[@]+"${rm_flags[@]}"} "$wt" 2>>"$(agents_log_file)" \
    || die "closed $s, but could not remove $wt. Details: $(agents_log_file)"
  unregister_worktree "$real"
  delete_branch
  say "closed $s and removed its worktree"
else
  say "closed $s"
fi
notify_change
