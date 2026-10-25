#!/usr/bin/env bash
# TPM entry point: default options, key bindings, and tmux hooks.
. "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/scripts/lib.sh"
S="$AGENTS_SCRIPTS"

default() { [ -n "$(t show-options -gqv "$1")" ] || t set-option -g "$1" "$2"; }

# Where other tools find new.sh (see "For other tools" in the README). Set on
# every load, so it follows the checkout when it moves.
t set-option -g @agents_scripts "$S"

default @agents_sidebar on
default @agents_sidebar_width 28
default @agents_min_width 100
default @agents_split 50
default @agents_launch_cmd "claude --remote-control"
default @agents_ide_cmd cursor
default @agents_ide_mode auto
default @agents_key_focus Tab
default @agents_key_toggle b
default @agents_key_new N
# prefix+o is tmux's "next pane"; keep it.
default @agents_key_ide O
# Same meaning as tmux's own ( and ), in tree order.
default @agents_key_next ')'
default @agents_key_prev '('
default @agents_key_attention a
# prefix + j, then 1-9 or a-z: the task with that label in the tree.
default @agents_key_jump j

t bind-key "$(opt @agents_key_focus Tab)" run-shell -b "'$S/action.sh' focus"
t bind-key "$(opt @agents_key_toggle b)" run-shell -b "'$S/sidebar-toggle.sh' toggle"
t bind-key "$(opt @agents_key_new N)" run-shell -b "'$S/action.sh' new"
t bind-key "$(opt @agents_key_ide O)" run-shell -b "'$S/action.sh' ide-current"
t bind-key "$(opt @agents_key_next ')')" run-shell -b "'$S/action.sh' next"
t bind-key "$(opt @agents_key_prev '(')" run-shell -b "'$S/action.sh' prev"
t bind-key "$(opt @agents_key_attention a)" run-shell -b "'$S/action.sh' attention"
t bind-key "$(opt @agents_key_jump j)" switch-client -T agents-jump
keys="$AGENTS_JUMP_KEYS"
while [ -n "$keys" ]; do
  k="${keys%"${keys#?}"}"; keys="${keys#?}"
  t bind-key -T agents-jump "$k" run-shell -b "'$S/action.sh' jump $k"
done

# Index 73 keeps these hooks apart from the user's own entries.
ensure_cmd="run-shell -b \"'$S/sidebar-toggle.sh' ensure '#{window_id}'\""
refresh_cmd="run-shell -b \"'$S/status.sh'; '$S/refresh.sh'\""
t set-hook -g 'session-created[73]' "$ensure_cmd"
t set-hook -g 'after-new-window[73]' "$ensure_cmd"
t set-hook -g 'client-resized[73]' "run-shell -b \"'$S/sidebar-toggle.sh' sync\""
t set-hook -g 'client-attached[73]' "run-shell -b \"'$S/sidebar-toggle.sh' sync\""
t set-hook -g 'window-resized[73]' "run-shell -b \"'$S/sidebar-toggle.sh' fit '#{window_id}'\""
t set-hook -g 'pane-exited[73]' "run-shell -b \"'$S/sidebar-toggle.sh' reap\""
t set-hook -g 'after-kill-pane[73]' "run-shell -b \"'$S/sidebar-toggle.sh' reap\""
# Looking at a window marks its finished turns as seen.
for h in client-focus-in client-session-changed session-window-changed; do
  t set-hook -g "${h}[73]" "run-shell -b \"'$S/state.sh' seen\""
done
for h in session-closed session-renamed window-renamed window-linked window-unlinked; do
  t set-hook -g "${h}[74]" "$refresh_cmd"
done

case "$(t show-options -gqv status-right)" in
  *@agents_status*) ;;
  *) t set-option -g status-right "#{@agents_status}$(t show-options -gqv status-right)" ;;
esac

"$S/status.sh"
[ "$(opt @agents_sidebar on)" = on ] && "$S/sidebar-toggle.sh" on
exit 0
