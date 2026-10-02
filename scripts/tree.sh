#!/usr/bin/env bash
# Prints the sidebar tree, one row per line: <target> TAB <display>.
# Targets: r:<repo>, s:<session>, w:<session>:<window index>.
# Sessions are grouped by @agent_repo. Windows are listed only when a session has more than one.
# Usage: tree.sh [sidebar pane]   (its session and window are shown in bold)
set -u
. "$(dirname "$0")/lib.sh"

own="$(t display-message -p -t "${1:-}" "#{session_name}${TAB}#{window_index}" 2>/dev/null)"

{
  t list-sessions -F "S${TAB}#{session_name}${TAB}#{@agent_repo}${TAB}#{@agent_pr}${TAB}#{@agent_pr_state}${TAB}#{session_windows}"
  t list-windows -a -F "W${TAB}#{session_name}${TAB}#{window_index}${TAB}#{window_name}"
  t list-panes -a -F "P${TAB}#{session_name}${TAB}#{window_index}${TAB}#{@agent_state}${TAB}#{@agents_sidebar_pane}"
} 2>/dev/null | awk -F'\t' \
  -v own_s="${own%%"$TAB"*}" -v own_w="${own#*"$TAB"}" \
  -v icon_repo="$(opt @agents_icon_repo '')" -v icon_win="$(opt @agents_icon_window '')" \
  "$AGENTS_AWK_RANK"'
  function isort(a, n,   i, j, v) {
    for (i = 2; i <= n; i++) { v = a[i]; j = i - 1; while (j > 0 && a[j] > v) { a[j + 1] = a[j]; j-- } a[j + 1] = v }
  }
  function icon(r, merged) {
    if (r == 5) return Y "◐" Z
    if (r == 4) return R "✕" Z
    if (r == 3) return G "●" Z
    if (merged) return B "✓" Z
    if (r == 2) return D "○" Z
    if (r == 1) return D "·" Z
    return " "
  }
  $1 == "S" { sn[++ns] = $2; repo[$2] = $3; pr[$2] = $4; prs[$2] = $5; nw[$2] = $6 }
  $1 == "W" { k = $2 SUBSEP $3; wname[k] = $4; wk[++nwk] = $2 SUBSEP sprintf("%06d", $3) }
  $1 == "P" && $5 != "1" { r = rank($4); k = $2 SUBSEP $3; if (r > wr[k]) wr[k] = r; if (r > sr[$2]) sr[$2] = r }
  END {
    Y = "\033[33m"; R = "\033[31m"; G = "\033[32m"; B = "\033[34m"; D = "\033[2m"; BO = "\033[1m"; Z = "\033[0m"
    for (i = 1; i <= ns; i++) {
      s = sn[i]; g = (repo[s] == "" ? "~" : repo[s])
      if (!(g in seen)) { seen[g] = 1; gl[++ng] = g }
      sk[i] = g SUBSEP s
    }
    isort(gl, ng); isort(sk, ns); isort(wk, nwk)

    for (gi = 1; gi <= ng; gi++) {
      g = gl[gi]
      printf "r:%s\t%s%s %s%s\n", g, BO, icon_repo, (g == "~" ? "sessions" : g), Z
      m = 0
      for (i = 1; i <= ns; i++) { split(sk[i], p, SUBSEP); if (p[1] == g) gs[++m] = p[2] }
      for (j = 1; j <= m; j++) {
        s = gs[j]; last = (j == m)
        name = (s == own_s ? BO s Z : s)
        extra = (pr[s] != "" ? " " D "#" pr[s] Z : "")
        printf "s:%s\t%s %s %s%s\n", s, (last ? "└" : "├"), icon(sr[s], prs[s] == "merged"), name, extra
        if (nw[s] < 2) continue
        c = 0
        for (x = 1; x <= nwk; x++) { split(wk[x], q, SUBSEP); if (q[1] == s) wl[++c] = q[2] + 0 }
        for (y = 1; y <= c; y++) {
          idx = wl[y]; k = s SUBSEP idx
          label = idx ":" wname[k]
          if (s == own_s && idx == own_w) label = BO label Z
          printf "w:%s:%s\t%s %s %s %s %s\n", s, idx, (last ? " " : "│"), (y == c ? "└" : "├"), icon(wr[k], 0), icon_win, label
        }
      }
    }
  }'
