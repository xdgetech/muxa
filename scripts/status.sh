#!/usr/bin/env bash
# Writes session counts per state into the global option @agents_status,
# which status-right shows as #{@agents_status}.
set -u
. "$(dirname "$0")/lib.sh"

line="$(t list-panes -a -F "#{session_name}${TAB}#{@agent_state}" 2>/dev/null | awk_states -F'\t' "$AGENTS_AWK_STATE"'
  { r = rank($2); if (r > best[$1]) best[$1] = r }
  END {
    for (s in best) n[best[s]]++
    out = ""
    for (r = 5; r >= 2; r--) if (n[r]) out = out sicon(r) " " n[r] " "
    printf "%s", out
  }')"

t set-option -g @agents_status "$line"
