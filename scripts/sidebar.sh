#!/usr/bin/env bash
# Runs inside a sidebar pane: fzf over tree.sh. refresh.sh pushes reloads
# through the pane's Unix socket; a slow timer covers a missed push.
set -u
. "$(dirname "$0")/lib.sh"

pane="${TMUX_PANE:?sidebar.sh must run inside a tmux pane}"
sock="$(agents_run_dir)/sb-${pane#%}.sock"
tree="'$AGENTS_SCRIPTS/tree.sh' $pane"
act="'$AGENTS_SCRIPTS/action.sh'"

t set-option -p -t "$pane" @agents_sidebar_pane 1
t set-option -p -t "$pane" @agents_sock "$sock"

(
  while sleep "$(opt @agents_refresh_fallback 10)"; do
    [ -S "$sock" ] && curl -s -m 1 --unix-socket "$sock" -XPOST http://localhost -d "reload($tree)" >/dev/null 2>&1
  done
) &
ticker=$!
trap 'kill "$ticker" 2>/dev/null; rm -f "$sock"' EXIT

# fzf exits on ctrl-c; start it again so the sidebar never dies.
while :; do
  rm -f "$sock"
  rows="$("$AGENTS_SCRIPTS/tree.sh" "$pane")"
  own="$(t display-message -p -t "$pane" '#{session_name}')"
  start="$(printf '%s\n' "$rows" | awk -F'\t' -v t="s:$own" '$1 == t { print NR; exit }')"

  printf '%s\n' "$rows" | fzf --listen="$sock" \
    --ansi --no-input --no-sort --track --id-nth=1 --delimiter='\t' --with-nth=2 \
    --layout=reverse --info=hidden --no-scrollbar --pointer='▌' --gutter=' ' \
    --footer="$(opt @agents_footer 'n new c win o ide x close')" \
    --bind "load:pos(${start:-1})+unbind(load)" \
    --bind "j:down,k:up,R:reload($tree)" \
    --bind "enter:execute-silent($act go {1})" \
    --bind "esc:execute-silent($act out),q:execute-silent($act out)" \
    --bind "n:execute-silent($act new {1}),c:execute-silent($act win {1})" \
    --bind "o:execute-silent($act ide {1}),r:execute-silent($act rename {1})" \
    --bind "x:execute-silent($act close {1}),X:execute-silent($act keep {1})"
  sleep 0.2
done
