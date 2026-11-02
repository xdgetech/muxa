#!/usr/bin/env bash
# Fact: a session made with new-session -d, as a task launcher does, gets
# default-size (80x24), not the size of the desktop client attached elsewhere.
# Its panes are split from 80x24 and stay that size while only ignore-size
# control clients watch it. The desktop sizes the window once it views it.
# Set MUXA_REPRO_PLUGIN to tmux-agents' agents.tmux to see its task layout.
set -euo pipefail
MUXA_SOCKET="muxa-repro-$$"
. "$(dirname "$0")/../lib/tmux.bash"
work="$(mktemp -d)"
trap 'mt_down; rm -rf "$work"' EXIT

home="$(mt new-session -d -P -F '#{session_id}' -x 200 -y 50 sh)"
mt set-option -g window-size latest
mt_desktop "$home" 200 50 30 &
desktop=$!
# The status line takes a row: the desktop's windows are 200x49.
wait_for "[ \"\$(window_size '$home')\" = 200x49 ]"

# 1. Detached: default-size, whatever the desktop client's size.
task="$(echo 'new-session -d -P -F "#{session_id}" sh' | mt_control "$home" | awk '/^\$/ { print; exit }')"
[ -n "$task" ] || fail "new-session -d printed no session id"
mt split-window -d -h -t "$task" sh
[ "$(window_size "$task")" = 80x24 ] || fail "a detached session is $(window_size "$task"), expected 80x24"
widths="$(mt list-panes -t "$task" -F '#{pane_width}' | tr '\n' ' ')"
pass "a detached session is 80x24 next to a 200x50 desktop terminal; its split panes are $widths"

# 2. An ignore-size control client watching it changes nothing.
{ sleep 1; } | mt -C attach-session -t "$task" -f ignore-size >/dev/null 2>&1 || true
[ "$(window_size "$task")" = 80x24 ] || fail "an ignore-size client sized the window to $(window_size "$task")"
pass "an ignore-size control client leaves it at 80x24"

# 3. -x and -y at creation set its size.
sized="$(mt new-session -d -P -F '#{session_id}' -x 200 -y 50 sh)"
[ "$(window_size "$sized")" = 200x50 ] || fail "new-session -d -x 200 -y 50 made $(window_size "$sized")"
pass "new-session -d -x 200 -y 50 makes a 200x50 window"

# 4. The desktop client sizes it once it views it.
mt switch-client -c "$(mt list-clients -F '#{client_name}' | head -1)" -t "$task"
wait_for "[ \"\$(window_size '$task')\" = 200x49 ]" || fail "the desktop client did not size the window"
pass "switching the desktop client to it gives it the desktop's 200x49"
kill "$desktop" 2>/dev/null || true
wait 2>/dev/null || true

[ -n "${MUXA_REPRO_PLUGIN:-}" ] || skip "MUXA_REPRO_PLUGIN not set; tmux-agents layout check skipped"
plugin_dir="$(cd "$(dirname "$MUXA_REPRO_PLUGIN")" && pwd)"
export TMUX_AGENTS_SOCKET="$MUXA_SOCKET"
mt run-shell "$MUXA_REPRO_PLUGIN"
git -C "$work" init -q repo
git -C "$work/repo" -c user.email=repro@example.com -c user.name=repro commit -q --allow-empty -m init
name="$("$plugin_dir/scripts/new.sh" "$work/repo" repro-task --no-switch --no-agent 2>&1 | tail -1)"
sleep 2
mt list-panes -s -t "=$name" -F '#{pane_width}x#{pane_height} role=#{@agents_role} sidebar=#{@agents_sidebar_pane}'
pass "tmux-agents' new.sh --no-switch lays out the task in 80x24 (above)"
