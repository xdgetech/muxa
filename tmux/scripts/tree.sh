#!/usr/bin/env bash
# shellcheck disable=SC2016  # awk programs: $ is awk's, not the shell's
# Prints the sidebar tree, one row per line: <target> TAB <display>.
# Targets: I: (the Inbox), i:<item>, g:<section>, r:<section>:<repo>, s:<session>,
# w:<session>:<window index>. Sections and repo groups are AGENTS_AWK_GROUP's.
# A folded section lists no sessions, and they get no jump label (AGENTS_JUMP_KEYS).
# Details of a row are in detail.sh.
# Usage: tree.sh [--all] [sidebar pane]   (--all ignores folds; the pane's session
#        gets a ▎ bar and lists its windows)
set -u
. "$(dirname "$0")/lib.sh"

folded="$(opt "$AGENTS_FOLD_OPT" '')"
if [ "${1:-}" = --all ]; then folded=""; shift; fi
own=""
[ -n "${1:-}" ] && own="$(t display-message -p -t "$1" "#{session_name}${TAB}#{window_index}" 2>/dev/null)"

# The Inbox section first (ADR 0033): targets I: and i:<item id>.
"$AGENTS_SCRIPTS/inbox.sh" rows 2>/dev/null

{
  t list-sessions -f "$AGENTS_VISIBLE" -F "S${TAB}#{session_name}${TAB}#{@agent_repo}${TAB}#{@agent_pr}${TAB}#{@agent_pr_state}${TAB}#{session_windows}${TAB}#{@muxa_task}${TAB}#{@muxa_ask}${TAB}#{session_path}"
  t list-windows -a -f "$AGENTS_VISIBLE" -F "W${TAB}#{session_name}${TAB}#{window_index}${TAB}#{window_name}"
  t list-panes -a -f "$AGENTS_VISIBLE" -F "P${TAB}#{session_name}${TAB}#{window_index}${TAB}#{@agent_state}${TAB}#{@agents_sidebar_pane}"
} 2>/dev/null | awk_states -F'\t' \
  -v own_s="${own%%"$TAB"*}" -v own_w="${own#*"$TAB"}" -v folded="$folded" \
  -v icon_repo="$(opt @agents_icon_repo '📂')" -v icon_win="$(opt @agents_icon_window '')" \
  -v icon_merged="$(opt @agents_icon_merged '✅')" -v keys="$AGENTS_JUMP_KEYS" \
  "$AGENTS_AWK_STATE$AGENTS_AWK_GROUP"'
  function isort(a, n,   i, j, v) {
    for (i = 2; i <= n; i++) { v = a[i]; j = i - 1; while (j > 0 && a[j] > v) { a[j + 1] = a[j]; j-- } a[j + 1] = v }
  }
  # Two columns wide either way, so names line up with and without an agent.
  function slot(r,   i) { i = sicon(r); return (i == "" ? "  " : i) }
  BEGIN { n = split(folded, f, " "); for (i = 1; i <= n; i++) fold[f[i]] = 1 }
  $1 == "S" {
    sn[++ns] = $2; pr[$2] = $4; prs[$2] = $5; nw[$2] = $6
    sec[$2] = section($3, $7, $8, $9); rg[$2] = repogroup(sec[$2], $3)
  }
  $1 == "W" { k = $2 SUBSEP $3; wname[k] = $4; wk[++nwk] = $2 SUBSEP sprintf("%06d", $3) }
  $1 == "P" && $5 != "1" { r = rank($4); k = $2 SUBSEP $3; if (r > wr[k]) wr[k] = r; if (r > sr[$2]) sr[$2] = r }
  END {
    C = "\033[36m"; D = "\033[2m"; BO = "\033[1m"; Z = "\033[0m"
    bar = C "▎" Z
    for (i = 1; i <= ns; i++) {
      s = sn[i]; x = sec[s]
      sk[i] = sorder(x) SUBSEP rg[s] SUBSEP s
      count[x]++
      if (sr[s] > best[x]) best[x] = sr[s]
      if (s == own_s) owns[x] = 1
      if (!((x, rg[s]) in seen)) { seen[x, rg[s]] = 1; groups[x]++ }
    }
    isort(sk, ns); isort(wk, nwk)

    cur = ""; curg = ""
    for (i = 1; i <= ns; i++) {
      split(sk[i], p, SUBSEP); s = p[3]; x = sec[s]; g = p[2]
      if (x != cur) {
        cur = x; curg = ""; shut = (x in fold)
        head = (shut ? "▸ " : "▾ ") BO slabel(x) Z " " D count[x] Z
        if (shut && best[x]) head = head " " sicon(best[x])
        printf "g:%s\t%s%s\n", x, (shut && (x in owns) ? bar : ""), head
      }
      if (shut) continue
      # A repo heading in Tasks and Sessions; "no repo" only next to a repo.
      if (g != curg) {
        curg = g
        if ((x == "tasks" || x == "sessions") && (g != "~" || groups[x] > 1))
          printf "r:%s:%s\t  %s%s\n", x, g, (icon_repo == "" || g == "~" ? "" : icon_repo " "), rlabel(g)
      }
      mine = (s == own_s)
      num = (++nth <= length(keys) ? D substr(keys, nth, 1) Z : " ")
      extra = (prs[s] == "merged" ? " " icon_merged : pr[s] != "" ? " " D "#" pr[s] Z : "")
      printf "s:%s\t%s%s %s %s%s\n", s, (mine ? bar : " "), num, slot(sr[s]), (mine ? BO s Z : s), extra
      if (!mine || nw[s] < 2) continue
      for (y = 1; y <= nwk; y++) {
        split(wk[y], q, SUBSEP); if (q[1] != s) continue
        idx = q[2] + 0; k = s SUBSEP idx
        label = (icon_win == "" ? "" : icon_win " ") idx " " wname[k]
        if (idx == own_w) label = BO C label Z
        printf "w:%s:%s\t%s    %s %s\n", s, idx, bar, slot(wr[k]), label
      }
    }
  }'
