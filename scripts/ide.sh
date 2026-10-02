#!/usr/bin/env bash
# Opens a task's worktree in the IDE. On a client that came in over SSH or mosh,
# the IDE must open on that machine, so this copies the remote-open command to
# its clipboard (OSC 52) instead.
# Usage: ide.sh <session>
set -u
. "$(dirname "$0")/lib.sh"

s="${1:?usage: ide.sh <session>}"
wt="$(target_opt "=$s:" @agent_worktree)"
[ -n "$wt" ] || wt="$(t display-message -p -t "=$s:" '#{pane_current_path}')"
[ -d "$wt" ] || die "no directory for $s"
ide="$(opt @agents_ide_cmd cursor)"

# True when an ancestor of the tmux client process is sshd, mosh-server or tailscaled.
client_is_remote() {
  local pid cmd
  pid="$(t display-message -p '#{client_pid}' 2>/dev/null)"
  for _ in 1 2 3 4 5 6 7 8 9 10; do
    [ -n "$pid" ] && [ "$pid" -gt 1 ] 2>/dev/null || return 1
    cmd="$(ps -o comm= -p "$pid" 2>/dev/null)"
    case "$cmd" in *sshd*|*mosh-server*|*tailscaled*) return 0 ;; esac
    pid="$(ps -o ppid= -p "$pid" 2>/dev/null | tr -d ' ')"
  done
  return 1
}

mode="$(opt @agents_ide_mode auto)"
if [ "$mode" = auto ]; then
  if client_is_remote; then mode=remote; else mode=local; fi
fi

if [ "$mode" = remote ]; then
  host="$(opt @agents_ide_host "$(hostname -s)")"
  line="$(printf '%s --remote ssh-remote+%s %q' "$ide" "$host" "$wt")"
  t set-buffer -w "$line" || die "could not copy. Run on your laptop: $line"
  say "copied, paste it in a laptop terminal: $line"
else
  command -v "${ide%% *}" >/dev/null || die "$ide not found. Set @agents_ide_cmd."
  # shellcheck disable=SC2086  # @agents_ide_cmd may carry flags
  (nohup $ide "$wt" >/dev/null 2>&1 &)
  say "opening $wt in $ide"
fi
