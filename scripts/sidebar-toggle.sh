#!/usr/bin/env bash
# Keeps one sidebar pane on the left of every window.
# Usage: sidebar-toggle.sh <on|off|toggle|ensure <window>|sync|reap>
#   sync: after a client resize or attach. Hides sidebars on a narrow client.
#   reap: closes windows where only a sidebar is left.
set -u
. "$(dirname "$0")/lib.sh"

width() { opt @agents_sidebar_width 28; }
is_on() { [ "$(opt @agents_sidebar on)" = on ]; }

too_narrow() {
  local w
  w="$(t display-message -p '#{client_width}' 2>/dev/null)"
  [ -n "$w" ] && [ "$w" -lt "$(opt @agents_min_width 100)" ]
}

has_sidebar() { t list-panes -t "$1" -F '#{@agents_sidebar_pane}' 2>/dev/null | grep -qx 1; }

sidebar_panes() {
  t list-panes -a -F "#{pane_id}${TAB}#{@agents_sidebar_pane}" 2>/dev/null | awk -F'\t' '$2 == "1" { print $1 }'
}

ensure() {
  local win="$1" lock p
  is_on || return 0
  has_sidebar "$win" && return 0
  # The hooks and `on` can race for the same window.
  lock="$(agents_run_dir)/ensure-${win#@}"
  mkdir "$lock" 2>/dev/null || return 0
  if ! has_sidebar "$win"; then
    p="$(t split-window -hbf -d -l "$(width)" -t "$win" -P -F '#{pane_id}' \
      "exec '$AGENTS_SCRIPTS/sidebar.sh'" 2>/dev/null)" && t set-option -p -t "$p" @agents_sidebar_pane 1
  fi
  rmdir "$lock"
}

ensure_all() {
  local w
  for w in $(t list-windows -a -F '#{window_id}' 2>/dev/null); do ensure "$w"; done
}

kill_all() {
  local p
  for p in $(sidebar_panes); do t kill-pane -t "$p"; done
}

reap() {
  local w
  for w in $(t list-panes -a -F "#{window_id}${TAB}#{@agents_sidebar_pane}" 2>/dev/null |
    awk -F'\t' '{ n[$1]++; if ($2 != "1") work[$1]++ } END { for (k in n) if (!work[k]) print k }'); do
    t kill-window -t "$w"
  done
}

case "${1:-}" in
  on) t set-option -g @agents_sidebar on; ensure_all ;;
  off) t set-option -g @agents_sidebar off; kill_all ;;
  toggle) if is_on; then "$0" off; else "$0" on; fi ;;
  ensure) [ -n "${2:-}" ] && ! too_narrow && ensure "$2" ;;
  sync)
    if too_narrow; then
      kill_all
    elif is_on; then
      ensure_all
      for p in $(sidebar_panes); do t resize-pane -t "$p" -x "$(width)"; done
    fi ;;
  reap) reap ;;
  *) echo "usage: sidebar-toggle.sh <on|off|toggle|ensure <window>|sync|reap>" >&2; exit 2 ;;
esac
exit 0
