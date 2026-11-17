#!/usr/bin/env bash
# A stand-in for an agent UI that redraws on resize with "clear scrollback" (CSI 3J),
# which makes tmux drop the pane's history. Prints 500 rows first.
i=1
while [ "$i" -le 500 ]; do printf 'agent %03d\n' "$i"; i=$((i + 1)); done
trap 'printf "\033[H\033[2J\033[3J"; echo "agent redrawn"' WINCH
while :; do sleep 1; done
