#!/usr/bin/env bash
# Prints the sidebar tree, one row per line: <target> TAB <display>.
# Targets: r:<repo>, s:<session>, w:<session>:<window index>.
# Sessions are grouped by @agent_repo. Details of a row are in detail.sh.
# Usage: tree.sh [sidebar pane]   (its session gets a ▎ bar and lists its windows)
set -u
. "$(dirname "$0")/lib.sh"

own=""
[ -n "${1:-}" ] && own="$(t display-message -p -t "$1" "#{session_name}${TAB}#{window_index}" 2>/dev/null)"

{
  t list-sessions -F "S${TAB}#{session_name}${TAB}#{@agent_repo}${TAB}#{@agent_pr}${TAB}#{@agent_pr_state}${TAB}#{session_windows}"
  t list-windows -a -F "W${TAB}#{session_name}${TAB}#{window_index}${TAB}#{window_name}"
  t list-panes -a -F "P${TAB}#{session_name}${TAB}#{window_index}${TAB}#{@agent_state}${TAB}#{@agents_sidebar_pane}"
} 2>/dev/null | awk_states -F'\t' \
  -v own_s="${own%%"$TAB"*}" -v own_w="${own#*"$TAB"}" \
  -v icon_repo="$(opt @agents_icon_repo '📂')" -v icon_win="$(opt @agents_icon_window '')" \
  -v icon_merged="$(opt @agents_icon_merged '✅')" \
  "$AGENTS_AWK_STATE"'
  function isort(a, n,   i, j, v) {
    for (i = 2; i <= n; i++) { v = a[i]; j = i - 1; while (j > 0 && a[j] > v) { a[j + 1] = a[j]; j-- } a[j + 1] = v }
  }
  # Two columns wide either way, so names line up with and without an agent.
  function slot(r,   i) { i = sicon(r); return (i == "" ? "  " : i) }
  $1 == "S" { sn[++ns] = $2; repo[$2] = $3; pr[$2] = $4; prs[$2] = $5; nw[$2] = $6 }
  $1 == "W" { k = $2 SUBSEP $3; wname[k] = $4; wk[++nwk] = $2 SUBSEP sprintf("%06d", $3) }
  $1 == "P" && $5 != "1" { r = rank($4); k = $2 SUBSEP $3; if (r > wr[k]) wr[k] = r; if (r > sr[$2]) sr[$2] = r }
  END {
    C = "\033[36m"; D = "\033[2m"; BO = "\033[1m"; Z = "\033[0m"
    bar = C "▎" Z
    for (i = 1; i <= ns; i++) {
      s = sn[i]; g = (repo[s] == "" ? "~" : repo[s])
      if (!(g in seen)) { seen[g] = 1; gl[++ng] = g }
      sk[i] = g SUBSEP s
    }
    isort(gl, ng); isort(sk, ns); isort(wk, nwk)

    for (gi = 1; gi <= ng; gi++) {
      g = gl[gi]
      printf "r:%s\t%s%s%s\n", g, (icon_repo == "" ? "" : icon_repo " "), BO (g == "~" ? "sessions" : g), Z
      for (i = 1; i <= ns; i++) {
        split(sk[i], p, SUBSEP); if (p[1] != g) continue
        s = p[2]; mine = (s == own_s)
        extra = (prs[s] == "merged" ? " " icon_merged : pr[s] != "" ? " " D "#" pr[s] Z : "")
        printf "s:%s\t%s%s %s%s\n", s, (mine ? bar : " "), slot(sr[s]), (mine ? BO s Z : s), extra
        if (!mine || nw[s] < 2) continue
        for (x = 1; x <= nwk; x++) {
          split(wk[x], q, SUBSEP); if (q[1] != s) continue
          idx = q[2] + 0; k = s SUBSEP idx
          label = (icon_win == "" ? "" : icon_win " ") idx " " wname[k]
          if (idx == own_w) label = BO C label Z
          printf "w:%s:%s\t%s  %s %s\n", s, idx, bar, slot(wr[k]), label
        }
      }
    }
  }'
