#!/usr/bin/env bash
# Keeps one sidebar pane on the left of every window.
# Adding, removing or resizing a sidebar keeps the other panes' widths in proportion.
# Usage: sidebar-toggle.sh <on|off|toggle|ensure <window>|fit <window>|sync|reap>
#   fit: after a window resize. Puts the sidebar back to its width.
#   sync: after a client resize or attach. Hides sidebars on a narrow client.
#   reap: closes windows where only a sidebar is left.
set -u
. "$(dirname "$0")/lib.sh"

width() { opt @agents_sidebar_width 28; }
is_on() { [ "$(opt @agents_sidebar on)" = on ]; }

# A control-mode client (tmux -C) has no screen: its width is not a real one.
is_control_client() { [ "$(t display-message -p '#{client_control_mode}' 2>/dev/null)" = 1 ]; }

too_narrow() {
  local w
  is_control_client && return 1
  w="$(t display-message -p '#{client_width}' 2>/dev/null)"
  [ -n "$w" ] && [ "$w" -lt "$(opt @agents_min_width 100)" ]
}

has_sidebar() { t list-panes -t "$1" -F '#{@agents_sidebar_pane}' 2>/dev/null | grep -qx 1; }

sidebar_panes() {
  t list-panes -a -F "#{pane_id}${TAB}#{@agents_sidebar_pane}" 2>/dev/null | awk -F'\t' '$2 == "1" { print $1 }'
}

# Prints "<pane> <width>" per work pane, left to right, when the work panes are
# one row of columns. Other layouts are left to tmux.
split_of() {
  t list-panes -t "$1" -F "#{@agents_sidebar_pane}${TAB}#{pane_id}${TAB}#{pane_width}${TAB}#{pane_left}${TAB}#{pane_top}${TAB}#{pane_height}" 2>/dev/null |
    awk -F'\t' '$1 != "1"' | sort -t "$TAB" -k4,4n |
    awk -F'\t' '
      { row[NR] = $2 " " $3; span[$5 FS $6] = 1 }
      END { n = 0; for (k in span) n++; if (n == 1 && NR > 1) for (i = 1; i <= NR; i++) print row[i] }'
}

# Gives the panes in a split_of snapshot their old shares of the new width.
# Usage: restore_split <window> <output of split_of before the change>
restore_split() {
  [ -n "$2" ] || return 0
  local now
  now="$(t list-panes -t "$1" -F "#{pane_id}${TAB}#{pane_width}" 2>/dev/null)"
  # Rounding the running total, not each pane, so repeated toggles do not drift.
  NOW="$now" awk '
    BEGIN { n = split(ENVIRON["NOW"], l, "\n"); for (i = 1; i <= n; i++) { split(l[i], f, "\t"); cur[f[1]] = f[2] } }
    { id[NR] = $1; old[NR] = $2; was += $2; is += cur[$1] }
    END {
      if (was == 0) exit
      for (i = 1; i < NR; i++) {
        sum += old[i]; to = int(sum * is / was + 0.5)
        printf "%s %d\n", id[i], to - prev; prev = to
      }
    }' <<<"$2" | while read -r p w; do
    t resize-pane -t "$p" -x "$w" 2>/dev/null
  done
  return 0
}

ensure() {
  local win="$1" lock p split
  is_on || return 0
  has_sidebar "$win" && return 0
  # The hooks and `on` can race for the same window.
  lock="$(agents_run_dir)/ensure-${win#@}"
  mkdir "$lock" 2>/dev/null || return 0
  if ! has_sidebar "$win"; then
    split="$(split_of "$win")"
    p="$(t split-window -hbf -d -l "$(width)" -t "$win" -P -F '#{pane_id}' \
      "exec '$AGENTS_SCRIPTS/sidebar.sh'" 2>/dev/null)" && t set-option -p -t "$p" @agents_sidebar_pane 1
    restore_split "$win" "$split"
  fi
  rmdir "$lock"
}

fit() {
  local win="$1" p split
  p="$(t list-panes -t "$win" -F "#{pane_id}${TAB}#{@agents_sidebar_pane}" 2>/dev/null | awk -F'\t' '$2 == "1" { print $1; exit }')"
  [ -n "$p" ] || return 0
  split="$(split_of "$win")"
  t resize-pane -t "$p" -x "$(width)"
  restore_split "$win" "$split"
}

ensure_all() {
  local w
  for w in $(t list-windows -a -F '#{window_id}' 2>/dev/null); do ensure "$w"; done
}

kill_all() {
  local p win split
  for p in $(sidebar_panes); do
    win="$(t display-message -p -t "$p" '#{window_id}')"
    split="$(split_of "$win")"
    t kill-pane -t "$p"
    restore_split "$win" "$split"
  done
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
  fit) [ -n "${2:-}" ] && fit "$2" ;;
  sync)
    is_control_client && exit 0
    if too_narrow; then
      kill_all
    elif is_on; then
      ensure_all
      for w in $(t list-windows -a -F '#{window_id}' 2>/dev/null); do fit "$w"; done
    fi ;;
  reap) reap ;;
  *) echo "usage: sidebar-toggle.sh <on|off|toggle|ensure <window>|fit <window>|sync|reap>" >&2; exit 2 ;;
esac
exit 0
