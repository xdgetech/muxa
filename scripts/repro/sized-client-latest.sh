#!/usr/bin/env bash
# Fact: with window-size latest (tmux's default), a control client that set a
# size with refresh-client -C and has no ignore-size flag sizes the window
# while it is the window's latest client, as a desktop terminal does.
# - attach-session and switch-client make it the latest client;
# - its own send-keys and a repeated refresh-client -C do not;
# - a key press on the desktop client takes the window back;
# - when it sets ignore-size again or detaches, the desktop's size returns;
# - refresh-client -C runs the window-resized hook, not client-resized.
# ADR 0020 builds on this.
set -euo pipefail
MUXA_SOCKET="muxa-repro-$$"
. "$(dirname "$0")/../lib/tmux.bash"
hooks="$(mktemp)"
trap 'rm -f "$hooks"; mt_down' EXIT

session="$(mt new-session -d -P -F '#{session_id}' -x 200 -y 50 sh)"
pane="$(mt display-message -p -t "$session" '#{pane_id}')"
default="$(mt show-options -gwv window-size)"
[ "$default" = latest ] || fail "the default window-size is '$default', expected latest"
pass "the default window-size is latest"

# The desktop client types "x" 5 s after it attached.
mt_desktop "$session" 160 40 30 5 &
desktop=$!
wait_for "[ \"\$(window_size '$session')\" = 160x39 ]"
pass "the desktop terminal (160x40, one status line) sizes the window to 160x39"

mt set-hook -g client-resized "run-shell \"echo client-resized >> '$hooks'\""
mt set-hook -g window-resized "run-shell \"echo window-resized >> '$hooks'\""

# 1. Attached without ignore-size and without a size: still the desktop's size.
mt_control_open 3 "$session"
sleep 0.5
[ "$(window_size "$session")" = 160x39 ] || fail "a control client with no size resized the window to $(window_size "$session")"
pass "a control client with no size leaves the window at 160x39"

# 2. With a size: attach made it the latest client, so the window takes its size.
echo 'refresh-client -C 100x30' >&3
wait_for "[ \"\$(window_size '$session')\" = 100x30 ]" || fail "refresh-client -C 100x30 left the window at $(window_size "$session")"
pass "after refresh-client -C 100x30 the window is 100x30 (control clients have no status line)"
wait_for "grep -q window-resized '$hooks'" || fail "no window-resized hook ran"
grep -q client-resized "$hooks" && fail "refresh-client -C ran the client-resized hook"
pass "the resize ran the window-resized hook and no client-resized hook"

echo "send-keys -t $pane -l ok" >&3
sleep 0.5
[ "$(window_size "$session")" = 100x30 ] || fail "the control client's send-keys changed the window to $(window_size "$session")"

# 3. The desktop types: it is the latest client now.
wait_for "[ \"\$(window_size '$session')\" = 160x39 ]" || fail "a desktop key press did not take the window back: $(window_size "$session")"
pass "a desktop key press takes the window back to 160x39"

# 4. Neither send-keys nor a repeated size from the control client takes it back.
echo "send-keys -t $pane -l ok" >&3
echo 'refresh-client -C 100x30' >&3
sleep 0.7
[ "$(window_size "$session")" = 160x39 ] || fail "send-keys or refresh-client -C made the control client latest: $(window_size "$session")"
pass "the control client's send-keys and refresh-client -C do not make it latest"

# 5. switch-client to its own session does.
echo "switch-client -t $session ; refresh-client -C 100x30" >&3
wait_for "[ \"\$(window_size '$session')\" = 100x30 ]" || fail "switch-client did not make the control client latest: $(window_size "$session")"
pass "switch-client -t <its session> makes it latest again: 100x30"

# 6. ignore-size again: the desktop's size returns. Setting the flag alone
# does not recalculate; the repeated refresh-client -C does.
echo 'refresh-client -f ignore-size ; refresh-client -C 100x30' >&3
wait_for "[ \"\$(window_size '$session')\" = 160x39 ]" || fail "ignore-size did not release the window: $(window_size "$session")"
pass "refresh-client -f ignore-size, then refresh-client -C, gives the window back: 160x39"
echo "refresh-client -f '!ignore-size' ; switch-client -t $session ; refresh-client -C 100x30" >&3
wait_for "[ \"\$(window_size '$session')\" = 100x30 ]" || fail "dropping ignore-size did not size the window: $(window_size "$session")"

# 7. Detaching gives the window back to the desktop.
exec 3>&-
wait_for "[ \"\$(window_size '$session')\" = 160x39 ]" || fail "the window stayed $(window_size "$session") after the control client left"
pass "when the sized control client detaches, the window returns to 160x39"
kill "$desktop" 2>/dev/null || true
wait 2>/dev/null || true
