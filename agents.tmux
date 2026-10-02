#!/usr/bin/env bash
# TPM entry point: default options, key bindings, and tmux hooks.
. "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/scripts/lib.sh"
S="$AGENTS_SCRIPTS"

default() { [ -n "$(t show-options -gqv "$1")" ] || t set-option -g "$1" "$2"; }

default @agents_sidebar on
default @agents_sidebar_width 28
default @agents_min_width 100
default @agents_split 50
default @agents_launch_cmd "claude --remote-control"
default @agents_ide_cmd cursor
default @agents_ide_mode auto
default @agents_icon_repo ""
default @agents_icon_window ""
default @agents_key_focus Tab
default @agents_key_toggle b
default @agents_key_new N
# prefix+o is tmux's "next pane"; keep it.
default @agents_key_ide O

t bind-key "$(opt @agents_key_focus Tab)" run-shell -b "'$S/action.sh' focus"
t bind-key "$(opt @agents_key_toggle b)" run-shell -b "'$S/sidebar-toggle.sh' toggle"
t bind-key "$(opt @agents_key_new N)" run-shell -b "'$S/action.sh' new"
t bind-key "$(opt @agents_key_ide O)" run-shell -b "'$S/action.sh' ide-current"

# Index 73 keeps these hooks apart from the user's own entries.
ensure_cmd="run-shell -b \"'$S/sidebar-toggle.sh' ensure '#{window_id}'\""
refresh_cmd="run-shell -b \"'$S/status.sh'; '$S/refresh.sh'\""
t set-hook -g 'session-created[73]' "$ensure_cmd"
t set-hook -g 'after-new-window[73]' "$ensure_cmd"
t set-hook -g 'client-resized[73]' "run-shell -b \"'$S/sidebar-toggle.sh' sync\""
t set-hook -g 'client-attached[73]' "run-shell -b \"'$S/sidebar-toggle.sh' sync\""
t set-hook -g 'pane-exited[73]' "run-shell -b \"'$S/sidebar-toggle.sh' reap\""
t set-hook -g 'after-kill-pane[73]' "run-shell -b \"'$S/sidebar-toggle.sh' reap\""
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
