#!/usr/bin/env bash
# shellcheck disable=SC2016  # awk programs: $ is awk's, not the shell's
# Prints the sidebar footer: details of the row under the cursor, then the key hints.
# Session or window: state and its age, branch, worktree, PR, window count.
# Section or repo: the repo's path, and how many sessions are in each state.
# Usage: detail.sh <target from tree.sh>
set -u
. "$(dirname "$0")/lib.sh"

target="${1:-}"
D=$'\033[2m' BO=$'\033[1m' Z=$'\033[0m'

# 45s, 4m, 2h, 3d
age() {
  local s=$(( $(date +%s) - $1 ))
  if [ "$s" -lt 60 ]; then printf '%ss' "$s"
  elif [ "$s" -lt 3600 ]; then printf '%sm' $((s / 60))
  elif [ "$s" -lt 86400 ]; then printf '%sh' $((s / 3600))
  else printf '%sd' $((s / 86400)); fi
}

# "<icon> US <word> US <state time>" of the most urgent pane in a tmux target.
# Unit separator (US), not tab: the icon is empty without an agent.
state_of() {
  t list-panes "$@" -F "#{@agent_state}${TAB}#{@agent_state_at}${TAB}#{@agents_sidebar_pane}" 2>/dev/null |
    awk_states -F'\t' "$AGENTS_AWK_STATE"'
      $3 != "1" { r = rank($1); if (n++ == 0 || r > best) { best = r; at = $2 } }
      END { printf "%s\037%s\037%s\n", sicon(best), sword(best), (best ? at : "") }'
}

# Line 1 from state_of output and a label.
headline() {
  local icon word at line
  IFS=$'\037' read -r icon word at <<EOF
$1
EOF
  line="${icon:+$icon }${BO}$2${Z} ${D}· $word"
  [ -n "$at" ] && line="$line · $(age "$at")"
  printf '%s%s\n' "$line" "$Z"
}

# Branch, worktree, PR and window count of a session.
session_lines() {
  local s="$1" where info pr prs nw extra=""
  if where="$(git_where "$(session_dir "$s")")"; then
    printf '%s %s\n' "$(opt @agents_icon_branch '⎇')" "${where%%"$TAB"*}"
    printf '%s%s %s%s\n' "$D" "📁" "$(basename "${where#*"$TAB"}")" "$Z"
  fi
  # Unit separator, not tab: read would merge empty tab-separated fields.
  info="$(t display-message -p -t "=$s:" "#{@agent_pr}"$'\037'"#{@agent_pr_state}"$'\037'"#{session_windows}")"
  IFS=$'\037' read -r pr prs nw <<EOF
$info
EOF
  [ -n "$pr" ] && extra="#$pr${prs:+ $prs}"
  [ "${nw:-1}" -gt 1 ] && extra="${extra:+$extra · }$nw windows"
  [ -n "$extra" ] && printf '%s%s%s\n' "$D" "$extra" "$Z"
  return 0
}

# <section> [repo group]: the sessions in it, by state.
group_lines() {
  local sec="$1" g="${2:-}"
  t list-sessions -f "$AGENTS_VISIBLE" -F "#{session_name}${TAB}#{@agent_repo}${TAB}#{@agent_repo_dir}${TAB}#{@muxa_task}${TAB}#{@muxa_ask}${TAB}#{session_path}" 2>/dev/null |
    awk -F'\t' -v sec="$sec" -v g="$g" "$AGENTS_AWK_GROUP"'
      { x = section($2, $4, $5, $6) }
      x == sec && (g == "" || repogroup(x, $2) == g) { print $1 "\t" (g == "" ? "" : $3) }' |
    while IFS="$TAB" read -r s dir; do
      printf '%s\t%s\t%s\n' "$s" "$dir" "$(state_of -s -t "=$s:" | cut -d$'\037' -f1)"
    done |
    awk_states -F'\t' -v home="$HOME" -v D="$D" -v Z="$Z" "$AGENTS_AWK_STATE"'
      { n++; if (dir == "" && $2 != "") dir = $2; if ($3 != "") c[$3]++ }
      END {
        if (dir != "") { if (index(dir, home) == 1) dir = "~" substr(dir, length(home) + 1); print D "📁 " dir Z }
        line = n " session" (n == 1 ? "" : "s")
        for (r = 5; r >= 1; r--) if (c[sicon(r)]) line = line " · " sicon(r) " " c[sicon(r)]
        print D line Z
      }'
}

case "$target" in
  i:* | I:) "$AGENTS_SCRIPTS/inbox.sh" detail "$target"; exit 0 ;;
  s:*) s="${target#s:}"
       headline "$(state_of -s -t "=$s:")" "$s"
       session_lines "$s" ;;
  w:*) rest="${target#w:}"; s="${rest%:*}"; idx="${rest##*:}"
       name="$(t display-message -p -t "=$s:$idx" '#{window_name}' 2>/dev/null)"
       headline "$(state_of -t "=$s:$idx")" "$s:$idx $name"
       session_lines "$s" ;;
  g:*) sec="${target#g:}"
       printf '%s%s%s\n' "$BO" "$(awk -v s="$sec" "$AGENTS_AWK_GROUP"'BEGIN { print slabel(s) }')" "$Z"
       group_lines "$sec"
       printf '%sEnter or space fold, ? help%s\n' "$D" "$Z"
       exit 0 ;;
  r:*) rest="${target#r:}"; sec="${rest%%:*}"; g="${rest#*:}"
       printf '%s %s%s%s\n' "$(opt @agents_icon_repo '📂')" "$BO" "$(awk -v g="$g" "$AGENTS_AWK_GROUP"'BEGIN { print rlabel(g) }')" "$Z"
       group_lines "$sec" "$g" ;;
esac
printf '%s%s%s\n' "$D" "$(opt @agents_footer 'n new c win o ide x close ? help')" "$Z"
