#!/usr/bin/env bash
# Runs inside a sidebar pane: fzf over tree.sh, with detail.sh in the footer.
# refresh.sh pushes reloads through the pane's Unix socket when the rows change;
# a slow timer covers a missed push.
set -u
. "$(dirname "$0")/lib.sh"

pane="${TMUX_PANE:?sidebar.sh must run inside a tmux pane}"
sock="$(agents_run_dir)/sb-${pane#%}.sock"
rows="$(sidebar_rows_file "$pane")"
S="$AGENTS_SCRIPTS"
refresh="'$S/refresh.sh' $pane"
detail="'$S/detail.sh'"
home="'$S/sidebar-pos.sh' $pane"
act="'$S/action.sh'"

t set-option -p -t "$pane" @agents_sidebar_pane 1
t set-option -p -t "$pane" @agents_sock "$sock"

(
  while sleep "$(opt @agents_refresh_fallback 10)"; do "$S/refresh.sh" "$pane"; done
) &
ticker=$!
# The Inbox cache: every sidebar's loop asks, inbox.sh refreshes it once per interval.
(
  while :; do "$S/inbox.sh" cache; sleep "$(opt @agents_inbox_refresh 5)"; done
) &
inbox=$!
trap 'kill "$ticker" "$inbox" 2>/dev/null; rm -f "$sock" "$rows"' EXIT

# 1-9 go to the numbered task, like prefix + j then the number. Letters stay tree keys.
jumps=""
for i in 1 2 3 4 5 6 7 8 9; do
  jumps="$jumps${jumps:+,}$i:execute-silent($act jump $i)+transform($home)"
done

# fzf exits on ctrl-c; start it again so the sidebar never dies.
# Leaving the sidebar puts its cursor back on its own session, so every sidebar
# you switch to has its cursor on the session you picked.
while :; do
  rm -f "$sock"
  "$S/tree.sh" "$pane" >"$rows"
  own="$(t display-message -p -t "$pane" '#{session_name}')"

  # --sync so start:pos lands after the rows load.
  fzf --listen="$sock" --sync <"$rows" \
    --ansi --no-input --no-sort --track --id-nth=1 --delimiter='\t' --with-nth=2 \
    --layout=reverse --info=hidden --no-scrollbar --pointer=' ' --gutter=' ' \
    --highlight-line --ellipsis='…' --footer-border=top \
    --color='bg+:236,fg+:-1:regular,pointer:-1,gutter:-1,footer:-1,footer-border:238' \
    --footer="$("$S/detail.sh" "s:$own")" \
    --bind "start:$("$S/sidebar-pos.sh" "$pane")" \
    --bind "focus:bg-transform-footer($detail {1})" \
    --bind "load:bg-transform-footer($detail {1})" \
    --bind "j:down,k:up,R:execute-silent($refresh)" \
    --bind "enter:execute-silent($act go {1})+transform($home)" \
    --bind "esc:execute-silent($act out)+transform($home)" \
    --bind "q:execute-silent($act out)+transform($home)" \
    --bind "n:execute-silent($act new {1}),c:execute-silent($act win {1})" \
    --bind "o:execute-silent($act ide {1}),r:execute-silent($act rename {1})" \
    --bind "x:execute-silent($act close {1}),X:execute-silent($act keep {1})" \
    --bind "ctrl-x:execute-silent($act force {1})" \
    --bind "$jumps"
  sleep 0.2
done
