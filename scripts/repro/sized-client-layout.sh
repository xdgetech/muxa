#!/usr/bin/env bash
# Fact: how a split window's panes follow when a sized control client resizes
# the window (ADR 0020). tmux spreads the change over side-by-side panes one
# column at a time, so each pane grows by about the same amount, not in
# proportion. A pane gets a given width only through the window's width.
# Set MUXA_REPRO_PLUGIN to tmux-agents' agents.tmux to see its layout follow:
# its window-resized hook puts the sidebar back to its width and keeps the
# work panes' shares.
set -euo pipefail
MUXA_SOCKET="muxa-repro-$$"
. "$(dirname "$0")/../lib/tmux.bash"
work="$(mktemp -d)"
trap 'mt_down; rm -rf "$work"' EXIT

widths() { mt list-panes -t "$1" -F '#{pane_width}' | tr '\n' ' ' | sed 's/ $//'; }

# A task session as a launcher makes it: 80x24, a 28-column pane on the left
# and two panes of 25 columns.
task="$(mt new-session -d -P -F '#{session_id}' sh)"
mt split-window -d -h -t "$task" sh
mt split-window -d -hbf -l 28 -t "$task" sh
[ "$(widths "$task")" = "28 25 25" ] || fail "the start layout is '$(widths "$task")', expected '28 25 25'"

mt_control_open 3 "$task"
echo 'refresh-client -C 180x50' >&3
wait_for "[ \"\$(window_size '$task')\" = 180x50 ]" || fail "the window is $(window_size "$task"), expected 180x50"
# 100 more columns, spread one by one over three panes: 34, 33, 33.
[ "$(widths "$task")" = "62 58 58" ] || fail "at 180x50 the panes are '$(widths "$task")', expected '62 58 58'"
pass "at 180 columns the panes are 62 58 58: each grew by about a third of the 100 new columns"
echo 'refresh-client -C 120x40' >&3
wait_for "[ \"\$(window_size '$task')\" = 120x40 ]"
[ "$(mt list-panes -t "$task" -F '#{pane_height}' | sort -u)" = 40 ] || fail "the panes are not 40 rows high"
pass "at 120x40 the panes are $(widths "$task") columns and 40 rows: rows follow the window"
exec 3>&-

[ -n "${MUXA_REPRO_PLUGIN:-}" ] || skip "MUXA_REPRO_PLUGIN not set; tmux-agents layout check skipped"
plugin_dir="$(cd "$(dirname "$MUXA_REPRO_PLUGIN")" && pwd)"
export TMUX_AGENTS_SOCKET="$MUXA_SOCKET"
mt run-shell "$MUXA_REPRO_PLUGIN"
git -C "$work" init -q repo
git -C "$work/repo" -c user.email=repro@example.com -c user.name=repro commit -q --allow-empty -m init
name="$("$plugin_dir/scripts/new.sh" "$work/repo" repro-task --no-switch --no-agent 2>&1 | tail -1)"
sleep 2
agents="$(mt display-message -p -t "=$name" '#{session_id}')"
echo "tmux-agents at $(window_size "$agents"): $(widths "$agents")"
mt_control_open 3 "$agents"
for size in 180x50 390x50 120x40; do
  echo "refresh-client -C $size" >&3
  sleep 1.5
  echo "tmux-agents at $(window_size "$agents"): $(widths "$agents")"
done
exec 3>&-
first="$(widths "$agents" | cut -d' ' -f1)"
[ "$first" = "$(mt show-options -gqv @agents_sidebar_width)" ] || fail "the sidebar is $first columns, not @agents_sidebar_width"
pass "tmux-agents keeps its sidebar width and splits the rest by the panes' shares (above)"
