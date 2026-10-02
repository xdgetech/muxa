#!/usr/bin/env bash
# Writes session counts per state into the global option @agents_status,
# which status-right shows as #{@agents_status}.
set -u
. "$(dirname "$0")/lib.sh"

line="$(t list-panes -a -F "#{session_name}${TAB}#{@agent_state}" 2>/dev/null | awk -F'\t' "$AGENTS_AWK_RANK"'
  { r = rank($2); if (r > best[$1]) best[$1] = r }
  END {
    for (s in best) n[best[s]]++
    out = ""
    if (n[5]) out = out "#[fg=yellow]◐ " n[5] " "
    if (n[4]) out = out "#[fg=red]✕ " n[4] " "
    if (n[3]) out = out "#[fg=green]● " n[3] " "
    if (out != "") out = out "#[default]"
    printf "%s", out
  }')"

t set-option -g @agents_status "$line"
