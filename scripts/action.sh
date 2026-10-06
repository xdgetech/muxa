#!/usr/bin/env bash
# What the sidebar keys and the prefix bindings do.
# Usage: action.sh <go|out|focus|new|win|ide|ide-current|close|keep|rename> [target]
# Targets come from tree.sh: r:<repo>, s:<session>, w:<session>:<window index>.
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

# Repo for a new task: the selected row's repo, else the current pane's.
repo_for() {
  local s dir
  case "$1" in
    r:*) dir="$(t list-sessions -F "#{@agent_repo}${TAB}#{@agent_repo_dir}" |
                awk -F'\t' -v g="${1#r:}" '$1 == g && $2 != "" { print $2; exit }')" ;;
    *) s="$(session_of "$1")" && dir="$(target_opt "=$s:" @agent_repo_dir)" ;;
  esac
  [ -n "${dir:-}" ] || dir="$(main_repo "$(t display-message -p '#{pane_current_path}')")"
  printf '%s' "${dir:-$HOME}"
}

new() {
  local repo; repo="$(repo_for "$target")"
  t command-prompt -I "$repo," -p "repo:,branch:" \
    "run-shell -b \"'$S/new.sh' '%1' '%2'\""
}

win() {
  local s wt w
  s="$(session_of "$target")" || return 0
  wt="$(target_opt "=$s:" @agent_worktree)"
  [ -n "$wt" ] || wt="$(t display-message -p -t "=$s:" '#{pane_current_path}')"
  w="$(t new-window -d -t "=$s:" -c "$wt" -P -F '#{window_id}')" || die "could not open a window in $s"
  "$S/layout.sh" "$w"
  t select-window -t "$w" && t switch-client -t "=$s"
}

case "$verb" in
  new|win|close|keep) leave_sidebar ;;
esac

case "$verb" in
  go) go ;;
  out) out ;;
  focus) focus ;;
  new) new ;;
  win) win ;;
  ide) s="$(session_of "$target")" && "$S/ide.sh" "$s" ;;
  ide-current) "$S/ide.sh" "$(t display-message -p '#{session_name}')" ;;
  close)
    s="$(session_of "$target")" || exit 0
    t confirm-before -p "close $s and remove its worktree? (y/n)" "run-shell -b \"'$S/close.sh' '$s'\"" ;;
  keep)
    s="$(session_of "$target")" || exit 0
    t confirm-before -p "close $s and keep its worktree? (y/n)" "run-shell -b \"'$S/close.sh' --keep '$s'\"" ;;
  rename)
    s="$(session_of "$target")" || exit 0
    t command-prompt -I "$s" -p "rename:" "rename-session -t '=$s' '%%'" ;;
  *) echo "usage: action.sh <go|out|focus|new|win|ide|ide-current|close|keep|rename> [target]" >&2; exit 2 ;;
esac
