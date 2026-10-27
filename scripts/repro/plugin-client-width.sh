#!/usr/bin/env bash
# Fact: a plugin hook that reads #{client_width} on client-attached sees 80 for
# a control client, unless the client sets a size with refresh-client -C in
# the same command line as the attach. Plugins that hide panes on narrow
# clients (tmux-agents' sidebar) then change the desktop layout.
#
# Generic check with a tiny hook; set MUXA_REPRO_PLUGIN to also run a real
# plugin entry file (e.g. tmux-agents' agents.tmux) and diff its layout.
set -euo pipefail
MUXA_SOCKET="muxa-repro-$$"
. "$(dirname "$0")/../lib/tmux.bash"
trap mt_down EXIT

session="$(mt new-session -d -P -F '#{session_id}' -x 200 -y 50 sh)"
log="${TMPDIR:-/tmp}/muxa-repro-$$.width"
: >"$log"
mt set-hook -g client-attached "run-shell -b \"sleep 0.2; tmux display-message -p '#{client_width}' >> '$log'\""

{ sleep 1; } | mt -C attach-session -t "$session" -f ignore-size >/dev/null 2>&1
{ sleep 1; } | mt -C attach-session -t "$session" -f ignore-size \; refresh-client -C 400x100 >/dev/null 2>&1
widths="$(tr '\n' ' ' <"$log")"
rm -f "$log"
[ "$widths" = "80 400 " ] || fail "hook saw widths '$widths', expected '80 400 '"
pass "a hook sees 80 columns for a plain control client and 400 with refresh-client -C at attach"

[ -n "${MUXA_REPRO_PLUGIN:-}" ] || skip "MUXA_REPRO_PLUGIN not set; real plugin check skipped"
mt set-hook -gu client-attached
mt run-shell "$MUXA_REPRO_PLUGIN"
sleep 1
before="$(mt list-panes -a -F '#{pane_id} #{pane_width}x#{pane_height}')"
{ sleep 1.5; } | mt -C attach-session -t "$session" -f ignore-size \; refresh-client -C 400x100 >/dev/null 2>&1
after="$(mt list-panes -a -F '#{pane_id} #{pane_width}x#{pane_height}')"
[ "$before" = "$after" ] || fail "plugin changed the layout: $before -> $after"
pass "with refresh-client -C at attach, the plugin left the layout unchanged"
