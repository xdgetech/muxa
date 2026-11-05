#!/usr/bin/env bash
# What the sidebar keys and the prefix bindings do.
# Usage: action.sh <go|out|focus|new|ask|win|ide|ide-current|close|keep|force|rename|fold> [target]
#        action.sh <next|prev|attention|help> | jump <key>
# Targets come from tree.sh: g:<section>, r:<section>:<repo>, s:<session>, w:<session>:<window index>.
set -u
. "$(dirname "$0")/lib.sh"

verb="${1:-}"
target="${2:-}"
S="$AGENTS_SCRIPTS"

session_of() {
  case "$1" in
    s:*) printf '%s' "${1#s:}" ;;
    w:*) local rest="${1#w:}"; printf '%s' "${rest%:*}" ;;
    *) return 1 ;;
  esac
}

# The pane to work in: the AI pane, else the first pane that is not a sidebar.
work_pane() {
  t list-panes ${1:+-t "$1"} -F "#{pane_id}${TAB}#{@agents_sidebar_pane}${TAB}#{@agents_role}" |
    awk -F'\t' '$2 != "1" { if ($3 == "ai") { print $1; found = 1; exit } if (first == "") first = $1 }
                END { if (!found && first != "") print first }'
}

sidebar_pane() {
  t list-panes -F "#{pane_id}${TAB}#{@agents_sidebar_pane}" | awk -F'\t' '$2 == "1" { print $1; exit }'
}

# Leaves the sidebar only after the switch, so the old window does not redraw
# before you see the new one.
go() {
  local s idx p old
  old="$(t display-message -p '#{?@agents_sidebar_pane,#{window_id},}')"
  case "$target" in
    i:* | I:)
      "$S/muxa.sh" inbox "$(t display-message -p '#{client_name}')" "${target#[iI]:}"
      return 0 ;;
    g:*) fold; return 0 ;;
    s:*) s="${target#s:}"; t switch-client -t "=$s" || return 1 ;;
    w:*) s="$(session_of "$target")"; idx="${target##*:}"
         t select-window -t "=$s:$idx" && t switch-client -t "=$s" || return 1 ;;
    *) return 0 ;;
  esac
  p="$(work_pane "=$s:")"
  [ -n "$p" ] && t select-pane -t "$p"
  # Not "#{window_id}" without -t: that is the sidebar's own window, from TMUX_PANE.
  [ -n "$old" ] && [ "$old" != "$(t display-message -p -t "=$s:" '#{window_id}')" ] && out "$old"
  return 0
}

# Before switching away, give the current window its work pane back, so
# returning to it does not land in the sidebar.
leave_sidebar() {
  [ "$(t display-message -p '#{@agents_sidebar_pane}')" = 1 ] && out
  return 0
}

# Out of the sidebar in <window>, default the current one: the last pane, else the work pane.
out() {
  local w="${1:-}" p
  t last-pane ${w:+-t "$w"} 2>/dev/null &&
    [ "$(t display-message -p ${w:+-t "$w"} '#{@agents_sidebar_pane}')" != 1 ] && return 0
  p="$(work_pane "$w")"
  [ -n "$p" ] && t select-pane -t "$p"
}

# prefix+Tab: into the sidebar, or back out of it.
focus() {
  if [ "$(t display-message -p '#{@agents_sidebar_pane}')" = 1 ]; then out; return; fi
  local p; p="$(sidebar_pane)"
  if [ -z "$p" ]; then
    "$S/sidebar-toggle.sh" on
    p="$(sidebar_pane)"
  fi
  [ -n "$p" ] && t select-pane -t "$p"
}

# Sessions in the order and numbering of the tree. --all: also those in folded sections.
tree_sessions() { "$S/tree.sh" "$@" | awk -F'\t' '$1 ~ /^s:/ { print substr($1, 3) }'; }

current_session() { t display-message -p '#{session_name}'; }

# Folds or unfolds the section of a g: row, in every sidebar.
fold() {
  local sec new
  case "$target" in g:*) sec="${target#g:}" ;; *) return 0 ;; esac
  new="$(awk -v cur="$(opt "$AGENTS_FOLD_OPT" '')" -v s="$sec" 'BEGIN {
    n = split(cur, a, " ")
    for (i = 1; i <= n; i++) if (a[i] == s) hit = 1; else out = out (out == "" ? "" : " ") a[i]
    if (!hit) out = out (out == "" ? "" : " ") s
    print out }')"
  t set-option -g "$AGENTS_FOLD_OPT" "$new"
  notify_change
}

# Every key, in a popup. The popup gets the server's environment, not this script's.
help() {
  local client; client="$(t display-message -p '#{client_name}')"
  set -- "$S/help.sh" view
  [ -n "${TMUX_AGENTS_SOCKET:-}" ] && set -- env "TMUX_AGENTS_SOCKET=$TMUX_AGENTS_SOCKET" "$@"
  t display-popup -c "$client" -E -w 76 -h 90% -T " muxa for tmux: keys " "$(printf '%q ' "$@")"
}

# Go to the session <n> rows after the current one in the tree, wrapping around.
step() {
  local s
  s="$(tree_sessions | awk -v cur="$(current_session)" -v d="$1" '
    { o[NR] = $0; if ($0 == cur) c = NR }
    END { if (NR) print o[c ? (c - 1 + d + NR) % NR + 1 : 1] }')"
  [ -n "$s" ] && target="s:$s" && go
}

# Go to the session the tree labels <key>.
jump() {
  local n s
  n="$(awk -v keys="$AGENTS_JUMP_KEYS" -v k="$1" 'BEGIN { print (length(k) == 1 ? index(keys, k) : 0) }')"
  [ "$n" -gt 0 ] || return 0
  s="$(tree_sessions | sed -n "${n}p")"
  [ -n "$s" ] || { say "no task $1"; return 0; }
  target="s:$s" && go
}

# Go to the most urgent session that needs you: needs input, then error, then
# finished, also in a folded section. Ties go to the first one after the current session.
attention() {
  local s
  s="$({
    t list-panes -a -f "$AGENTS_VISIBLE" -F "R${TAB}#{session_name}${TAB}#{@agent_state}"
    tree_sessions --all | sed "s/^/O${TAB}/"
  } | awk -F'\t' -v cur="$(current_session)" "$AGENTS_AWK_STATE"'
    $1 == "R" { r = rank($3); if (r > best[$2]) best[$2] = r; next }
    { o[++n] = $2; if ($2 == cur) c = n }
    END {
      for (i = 1; i <= n; i++) {
        x = o[(c + i - 1) % n + 1]; r = best[x]
        if ((r == 5 || r == 4 || r == 2) && r > top) { top = r; pick = x }
      }
      print pick
    }')"
  [ -n "$s" ] || { say "no task needs you"; return 0; }
  target="s:$s" && go
}

# Repo for a new task: the selected row's repo, else the current pane's.
repo_for() {
  local s dir
  case "$1" in
    r:*) s="${1#r:}"
         dir="$(t list-sessions -f "$AGENTS_VISIBLE" -F "#{@agent_repo}${TAB}#{@agent_repo_dir}" |
                awk -F'\t' -v g="${s#*:}" '$1 == g && $2 != "" { print $2; exit }')" ;;
    *) s="$(session_of "$1")" && dir="$(target_opt "=$s:" @agent_repo_dir)" ;;
  esac
  [ -n "${dir:-}" ] || dir="$(main_repo "$(t display-message -p '#{pane_current_path}')")"
  printf '%s' "${dir:-$HOME}"
}

new() {
  local repo; repo="$(repo_for "$target")"
  # A muxa task when muxa is installed, so every device sees it.
  if "$S/muxa.sh" available; then
    "$S/muxa.sh" new "$(t display-message -p '#{client_name}')" "$repo"
    return
  fi
  t command-prompt -I "$repo," -p "repo:,branch:" \
    "run-shell -b \"'$S/new.sh' '%1' '%2'\""
}

# An ask session through muxa, in the current repo by default.
ask() { "$S/muxa.sh" ask "$(t display-message -p '#{client_name}')" "$(repo_for "$target")"; }

win() {
  local s wt w
  s="$(session_of "$target")" || return 0
  wt="$(target_opt "=$s:" @agent_worktree)"
  [ -n "$wt" ] || wt="$(t display-message -p -t "=$s:" '#{pane_current_path}')"
  w="$(t new-window -d -t "=$s:" -c "$wt" -P -F '#{window_id}')" || die "could not open a window in $s"
  "$S/layout.sh" "$w"
  t select-window -t "$w" && t switch-client -t "=$s"
}

# A session that runs a muxa Task also asks what becomes of the Task, so a
# closed session does not leave it open in the Inbox. <mode> is close.sh's flag.
# A session without a worktree (an ask or manual session) only closes.
close_session() {
  local mode="$1" what="$2" s run
  s="$(session_of "$target")" || return 0
  run="run-shell -b \"'$S/close.sh' $mode"
  what=" and $what"
  [ -n "$(target_opt "=$s:" @agent_worktree)" ] || what=""
  if [ -z "$(target_opt "=$s:" @muxa_task)" ] || ! "$S/muxa.sh" available; then
    t confirm-before -p "close $s$what? (y/n)" "$run '$s'\""
    return
  fi
  t display-menu -x C -y C -T "#[align=centre] close $s$what " \
    "Close, mark the Task done" d "$run --task done '$s'\"" \
    "Close, drop the Task" x "$run --task dropped '$s'\"" \
    "Close, keep the Task open" k "$run '$s'\"" \
    "" \
    "Cancel" q ""
}

case "$verb" in
  new|win|close|keep|force) leave_sidebar ;;
esac

case "$verb" in
  go) go ;;
  next) step 1 ;;
  prev) step -1 ;;
  jump) jump "$target" ;;
  attention) attention ;;
  out) out ;;
  focus) focus ;;
  new) new ;;
  ask) ask ;;
  win) win ;;
  ide) s="$(session_of "$target")" && "$S/ide.sh" "$s" ;;
  ide-current) "$S/ide.sh" "$(t display-message -p '#{session_name}')" ;;
  close) close_session "" "remove its worktree" ;;
  keep) close_session --keep "keep its worktree" ;;
  force) close_session --force "remove its worktree, even if unmerged" ;;
  fold) fold ;;
  help) help ;;
  rename)
    s="$(session_of "$target")" || exit 0
    t command-prompt -I "$s" -p "rename:" "rename-session -t '=$s' '%%'" ;;
  *) echo "usage: action.sh <go|out|focus|new|ask|win|ide|ide-current|close|keep|force|rename|fold> [target] | <next|prev|attention|help> | jump <key>" >&2; exit 2 ;;
esac
