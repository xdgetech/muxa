#!/usr/bin/env bash
# Closes a task session. Without --keep it also removes the worktree, but only
# when nothing can be lost: no uncommitted changes, and the branch is merged.
# --force skips the merged check. The branch itself is always kept.
# In every mode it stops the processes and compose projects left in the worktree.
# Usage: close.sh [--keep|--force] <session>
set -u
. "$(dirname "$0")/lib.sh"

keep=0 force=0
case "${1:-}" in
  --keep) keep=1; shift ;;
  --force) force=1; shift ;;
esac
s="${1:-}"
[ -n "$s" ] || die "usage: close.sh [--keep|--force] <session>"
t has-session -t "=$s" 2>/dev/null || die "no session $s"

wt="$(target_opt "=$s:" @agent_worktree)"
main="$(target_opt "=$s:" @agent_repo_dir)"
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

remove_wt=0
if [ "$keep" = 0 ] && [ -n "$wt" ] && [ -d "$wt" ]; then
  [ -z "$(git -C "$wt" status --porcelain 2>/dev/null)" ] \
    || die "$s has uncommitted changes. Commit or stash them, or press X to close and keep the worktree."
  [ "$force" = 1 ] || merged || die "$s is not merged. Press X to close and keep the worktree, or ctrl-x to remove it anyway."
  remove_wt=1
fi

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
[ -n "$wt" ] && [ -d "$wt" ] && "$AGENTS_SCRIPTS/stop-worktree.sh" "$wt"
if [ "$remove_wt" = 1 ]; then
  git -C "${main:-$wt}" worktree remove "$wt" 2>>"$(agents_log_file)" \
    || die "closed $s, but could not remove $wt. Details: $(agents_log_file)"
  say "closed $s and removed its worktree"
else
  say "closed $s"
fi
notify_change
