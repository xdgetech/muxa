#!/usr/bin/env bash
# Prints the fzf action that puts a sidebar's cursor on its own session: pos(<row>).
# Reads the rows the sidebar last loaded, so the row number matches fzf's list.
# Usage: sidebar-pos.sh <sidebar pane>
set -u
. "$(dirname "$0")/lib.sh"

pane="${1:?usage: sidebar-pos.sh <sidebar pane>}"
own="$(t display-message -p -t "$pane" '#{session_name}' 2>/dev/null)"
row="$(awk -F'\t' -v t="s:$own" '$1 == t { print NR; exit }' "$(sidebar_rows_file "$pane")" 2>/dev/null)"
printf 'pos(%s)' "${row:-1}"
