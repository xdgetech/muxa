#!/usr/bin/env bash
# Fact: a device session, a session of its own that holds one window of the
# user's session through link-window (ADR 0021), and what it changes.
# - new-session -d, link-window -d, then kill-window of its first window: it
#   holds only that window. The user's session keeps all its windows, its
#   current window, and its clients their view. Hooks: session-created (with
#   the first window), window-linked, window-unlinked, session-closed.
# - list-sessions, list-windows -a and list-panes -a show it; a session option
#   hides it from a -f filter, also in the window and pane lists.
# - A command with no target picks the session switched to last.
# - kill-session of it changes nothing in the user's session. kill-session of
#   the user's session leaves the linked window alive in it.
# - A grouped session (new-session -t) cannot hold one window: it shares all.
set -euo pipefail
MUXA_SOCKET="muxa-repro-$$"
. "$(dirname "$0")/../lib/tmux.bash"
hooks="$(mktemp)"
trap 'exec 3>&- 4>&- 5>&- 2>/dev/null || true; rm -f "$hooks"; mt_down' EXIT

target="$(mt new-session -d -P -F '#{session_id}' -x 160 -y 39 sh)"
first="$(mt display-message -p -t "$target" '#{window_id}')"
second="$(mt new-window -d -P -F '#{window_id}' -t "$target" sh)"
pane="$(mt display-message -p -t "$second" '#{pane_id}')"
mt split-window -d -h -t "$second" sh
exec 5> >(mt_desktop "$target" 160 40 60 stdin)
# Created at the desktop's size, so no layout changes when it attaches.
wait_for "[ -n \"\$(mt list-clients -F x)\" ]"
desktop_view() { mt list-clients -F '#{client_session} #{window_id} #{pane_id} #{client_flags}' | grep -v control-mode; }
windows() { mt list-windows -t "$1" -F '#{window_id}' | tr '\n' ' '; }
layouts() { mt list-windows -t "$target" -F '#{window_id} #{window_layout}'; }
before="$(desktop_view)"
layout_before="$(layouts)"
for h in session-created session-closed window-linked window-unlinked; do
  mt set-hook -g "$h" "run-shell \"echo $h #{window_id} >> '$hooks'\""
done

# 1. One window, linked; the user's session and the desktop stay.
out="$(mt new-session -d -P -F '#{session_id} #{window_id}' -s _muxa-repro cat)"
device="${out% *}"
dummy="${out#* }"
# The hook expands #{window_id} when it runs, which tmux may defer past the
# next command: on a loaded machine it can then see the linked window.
wait_for "grep -q 'session-created' '$hooks'" || fail "no session-created hook ran"
mt link-window -d -s "$second" -t "$device:" \; kill-window -t "$dummy"
[ "$(windows "$device")" = "$second " ] || fail "the device session holds $(windows "$device"), expected $second"
[ "$(mt display-message -p -t "$device" '#{window_id}')" = "$second" ] || fail "the linked window is not the device session's current window"
[ "$(windows "$target")" = "$first $second " ] || fail "the user's session lost windows: $(windows "$target")"
[ "$(mt display-message -p -t "$target" '#{window_id}')" = "$first" ] || fail "the user's current window moved"
[ "$(desktop_view)" = "$before" ] || fail "the desktop client moved: $(desktop_view)"
[ "$(layouts)" = "$layout_before" ] || fail "a layout changed: $(layouts)"
wait_for "grep -q 'session-created $dummy' '$hooks' && grep -q 'window-linked $second' '$hooks'" || fail "hooks: $(cat "$hooks")"
pass "new-session -d, link-window -d and kill-window of its first window give a session that holds only $second; the user's session, its current window, the desktop and the layouts stay; session-created (first window) and window-linked run"

# 2. Lists show it; a session option hides it.
mt set-option -t "$device" @muxa_device 1
hide='#{==:#{@muxa_device},}'
[ "$(mt list-panes -a -F '#{pane_id}' | grep -cx "$pane")" = 2 ] || fail "list-panes -a does not list the linked pane twice"
[ "$(mt list-sessions -f "$hide" -F '#{session_id}')" = "$target" ] || fail "the filter did not hide it from list-sessions"
[ "$(mt list-windows -a -f "$hide" -F '#{window_id}' | grep -cx "$second")" = 1 ] || fail "the filter did not hide it from list-windows -a"
[ "$(mt list-panes -a -f "$hide" -F '#{pane_id}' | grep -cx "$pane")" = 1 ] || fail "the filter did not hide it from list-panes -a"
pass "list-sessions, list-windows -a and list-panes -a list it; -f '$hide' hides it (a session option is seen from window and pane formats)"

# 3. A command with no target picks the session switched to last.
mt_control_open 3 "$device" ignore-size,no-output
mt_control_open 4 "$target" ignore-size
mt_control_sync 3
mt_control_sync 4
echo "switch-client -t $device" >&3
mt_control_sync 3
[ "$(mt display-message -p '#{session_id}')" = "$device" ] || fail "the best session is not the device session after a switch to it"
echo "switch-client -t $target" >&4
mt_control_sync 4
[ "$(mt display-message -p '#{session_id}')" = "$target" ] || fail "a switch-client to the user's session did not make it the best session again"
pass "with no target, tmux picks the session switched to last: the device session, until a client switches to the user's session again"

# 4. kill-session of it, with its client attached: the user's session stays.
# The user's detach-on-destroy off would move that client to another session;
# the device session's own detach-on-destroy on detaches it.
controls() { mt list-clients -F '#{client_control_mode}' | grep -c 1; }
mt set-option -g detach-on-destroy off
mt set-option -t "$device" detach-on-destroy on
[ "$(controls)" = 2 ] || fail "expected two control clients, found $(controls)"
mt kill-session -t "$device"
wait_for "[ \"\$(controls)\" = 1 ]" || fail "the device session's client was not detached"
exec 3>&-
wait_for "grep -q 'session-closed' '$hooks'" || fail "no session-closed hook ran"
pass "with detach-on-destroy on set on the device session, killing it detaches its client, also when the user set detach-on-destroy off"
out="$(mt new-session -d -P -F '#{session_id} #{window_id}' cat)"
device="${out% *}"
mt link-window -d -s "$second" -t "$device:" \; kill-window -t "${out#* }"
mt_control_open 3 "$device" ignore-size,no-output
wait_for "[ \"\$(controls)\" = 2 ]"
mt kill-session -t "$device"
# A detached client would not answer.
mt_control_sync 3 || fail "without its own detach-on-destroy, the client was detached"
[ "$(controls)" = 2 ] || fail "without its own detach-on-destroy, the client was detached"
exec 3>&-
mt set-option -gu detach-on-destroy
pass "without it, the user's detach-on-destroy off moves the client to another session"
[ "$(windows "$target")" = "$first $second " ] || fail "killing the device session changed the user's windows: $(windows "$target")"
[ "$(layouts)" = "$layout_before" ] || fail "a layout changed: $(layouts)"
[ "$(desktop_view)" = "$before" ] || fail "the desktop client moved: $(desktop_view)"
pass "kill-session of the device session, with its client attached, leaves the user's windows, layouts and desktop client as they were"

# 5. kill-session of the user's session leaves the linked window alive.
out="$(mt new-session -d -P -F '#{session_id} #{window_id}' cat)"
device="${out% *}"
mt link-window -d -s "$second" -t "$device:" \; kill-window -t "${out#* }"
other="$(mt new-session -d -P -F '#{session_id}' sh)"
mt switch-client -c "$(mt list-clients -F '#{client_name}' -f '#{!=:#{client_control_mode},1}' | head -1)" -t "$other"
mt kill-session -t "$target"
[ "$(mt list-panes -s -t "$device" -F '#{pane_id}' | grep -cx "$pane")" = 1 ] || fail "the pane died with the user's session"
pass "kill-session of the user's session keeps its linked window alive in the device session: closing a session must close its device sessions first"
mt kill-session -t "$device"
if mt list-panes -a -F '#{pane_id}' | grep -qx "$pane"; then fail "the pane outlived its last session"; fi

# 6. A grouped session shares every window: it cannot hold one.
target="$(mt new-session -d -P -F '#{session_id}' sh)"
mt new-window -d -t "$target" sh
grouped="$(mt new-session -d -P -F '#{session_id}' -t "$target")"
[ "$(windows "$grouped")" = "$(windows "$target")" ] || fail "the grouped session's windows differ"
pass "a grouped session (new-session -t) holds every window of its target: $(windows "$grouped")"

# 7. With tmux-agents loaded: its hooks for a device session change no pane.
[ -n "${MUXA_REPRO_PLUGIN:-}" ] || skip "MUXA_REPRO_PLUGIN not set; tmux-agents hook check skipped"
work="$(mktemp -d)"
trap 'exec 3>&- 4>&- 5>&- 2>/dev/null || true; rm -f "$hooks"; rm -rf "$work"; mt_down' EXIT
plugin_dir="$(cd "$(dirname "$MUXA_REPRO_PLUGIN")" && pwd)"
export TMUX_AGENTS_SOCKET="$MUXA_SOCKET"
for h in session-created session-closed window-linked window-unlinked; do mt set-hook -gu "$h"; done
mt run-shell "$MUXA_REPRO_PLUGIN"
git -C "$work" init -q repo
git -C "$work/repo" -c user.email=repro@example.com -c user.name=repro commit -q --allow-empty -m init
"$plugin_dir/scripts/new.sh" "$work/repo" repro-task --no-switch --no-agent >/dev/null 2>&1
agents="$(mt list-sessions -f '#{==:#{session_name},repro-task}' -F '#{session_id}')"
[ -n "$agents" ] || fail "tmux-agents' new.sh made no session repro-task"
watched="$(mt new-window -d -P -F '#{window_id}' -t "$agents" sh)"
sleep 2
panes_of() { mt list-panes -s -t "$1" -F '#{window_id} #{pane_id} #{pane_width}x#{pane_height}' | sort; }
before="$(panes_of "$agents")"
out="$(mt new-session -d -P -F '#{session_id} #{window_id}' cat)"
device="${out% *}"
mt link-window -d -s "$watched" -t "$device:" \; kill-window -t "${out#* }" \; set-option -t "$device" @muxa_device 1
mt_control_open 3 "$device" ignore-size,no-output
echo "switch-client -t $device" >&3
sleep 2
[ "$(panes_of "$agents")" = "$before" ] || fail "tmux-agents changed the panes for the device session: $(panes_of "$agents")"
mt kill-session -t "$device"
exec 3>&-
sleep 2
[ "$(panes_of "$agents")" = "$before" ] || fail "tmux-agents changed the panes after the device session closed: $(panes_of "$agents")"
pass "tmux-agents' hooks for a device session add, remove and resize no pane of the user's session"
