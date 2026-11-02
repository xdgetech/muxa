#!/usr/bin/env bash
# Fact: the daemon can tell whether a person is typing (ADR 0019).
# A desktop client's key press updates its #{client_activity}; keys sent by
# a control client do not; #{client_flags} names control clients.
set -euo pipefail
MUXA_SOCKET="muxa-repro-$$"
. "$(dirname "$0")/../lib/tmux.bash"
trap mt_down EXIT

session="$(mt new-session -d -P -F '#{session_id}' -x 200 -y 50 cat)"
pane="$(mt display-message -p -t "$session" '#{pane_id}')"
# Lines of "<activity> <session> <flags>" for each client, as the daemon reads them.
clients() { mt list-clients -F '#{client_activity} #{client_session} #{client_flags}'; }
desktop_activity() { clients | grep -v control-mode | cut -d' ' -f1; }

# The desktop client types "x" 4 s after it attached.
mt_desktop "$session" 160 40 10 4 &
desktop=$!
wait_for "[ -n \"\$(desktop_activity)\" ]"
first="$(desktop_activity)"

# 3. A control client is named in client_flags; the desktop client is not.
listed="$({ echo "list-clients -F '#{client_flags}'"; sleep 0.5; } \
  | mt -C attach-session -t "$session" -f ignore-size,no-output 2>&1 || true)"
echo "$listed" | grep -q 'control-mode' || fail "client_flags does not name the control client: $listed"
clients | grep -v control-mode | grep -q . || fail "no desktop client listed"
pass "#{client_flags} names control clients (control-mode)"

# 2. Keys from a control client leave the desktop client's activity alone.
sleep 1.5
{ echo "send-keys -t $pane -l hello"; echo "send-keys -t $pane Enter"; sleep 0.5; } \
  | mt -C attach-session -t "$session" -f ignore-size,no-output >/dev/null 2>&1 || true
[ "$(desktop_activity)" = "$first" ] || fail "a control client's send-keys updated the desktop client's activity: $first -> $(desktop_activity)"
pass "a control client's send-keys does not update another client's #{client_activity}"

# 1. The desktop client's own key press does.
wait_for "[ \"\$(desktop_activity)\" -gt $first ]" \
  || fail "a desktop key press did not update #{client_activity} ($first): $(clients) at $(date +%s); pane: $(mt capture-pane -p -t "$pane" | head -3)"
pass "a desktop client's key press updates its #{client_activity}"
kill "$desktop" 2>/dev/null || true
wait 2>/dev/null || true
