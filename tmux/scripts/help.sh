#!/usr/bin/env bash
# Every key of muxa for tmux, with the keys the options set. `?` in the tree
# shows it in a popup (action.sh help).
# Usage: help.sh [print|view]
set -u
. "$(dirname "$0")/lib.sh"

BO=$'\033[1m' D=$'\033[2m' Z=$'\033[0m'

heading() { printf '\n%s%s%s\n' "$BO" "$1" "$Z"; }
# One key and what it does, keys in a column.
row() { printf '  %-14s %s\n' "$1" "$2"; }

print() {
  local p
  p="$(t show-options -gqv prefix 2>/dev/null)"
  p="${p:-C-b}"
  printf '%smuxa for tmux%s  %sq closes this%s\n' "$BO" "$Z" "$D" "$Z"

  heading "Anywhere: $p, then"
  row "$(opt @agents_key_focus Tab)" "into the tree, and back out"
  row "$(opt @agents_key_toggle b)" "show or hide the tree"
  row "$(opt @agents_key_new N)" "capture a thought (s save, a ask the assistant), or a new Task"
  row "$(opt @agents_key_ask A)" "ask session: an agent in a repo's main checkout"
  row "$(opt @agents_key_inbox T)" "the Inbox"
  row "$(opt @agents_key_ide O)" "open the current session in the IDE"
  row "$(opt @agents_key_next ')') $(opt @agents_key_prev '(')" "next / previous session, in tree order"
  row "$(opt @agents_key_jump j) 1-9, a-z" "the session with that label"
  row "$(opt @agents_key_attention a)" "the session that needs you most"

  heading "In the tree"
  row "j k" "move"
  row "Enter" "go to the row; on a section, fold or unfold it"
  row "space" "fold or unfold a section"
  row "1-9" "go to that session"
  row "n" "new Task in the row's repo"
  row "c" "new window in the session's worktree"
  row "o" "open the session in the IDE"
  row "r" "rename the session"
  row "x" "close the session, remove its worktree and branch; asks when work could be lost"
  row "X" "park the Task: close the session, keep the Task and its worktree"
  row "R" "reload"
  row "?" "this help"
  row "Esc q" "leave the tree"
  printf '  %sA Task session asks what becomes of its Task when it closes.%s\n' "$D" "$Z"

  heading "Sections (fold with Enter or space)"
  row "Tasks" "muxa Tasks, by repo"
  row "Routines" "Routine runs while they run"
  row "Ask" "ask sessions ($p $(opt @agents_key_ask A))"
  row "Sessions" "every other session, by repo"

  heading "A manual session"
  row "$p :" "then: new -s NAME -c DIR"
  printf '  %sIt lists under Sessions, and x closes it.%s\n' "$D" "$Z"

  heading "The Inbox ($p $(opt @agents_key_inbox T))"
  printf '  %sEach item lists the keys it takes. Enter goes to its session.%s\n' "$D" "$Z"
  row "l d a" "on a Work item: Later, Complete, ask its assistant"
  row "P" "on a Work item: pin it first, or unpin it"
  row "o" "on a Today row: go to its Work item or Inbox item"
  row "t" "ask the overall assistant for a plan now"
  row "K J" "on a Today row: move it up or down"
  row "x" "on a Today row: Not today, with an optional reason"
  row "y" "the Policies list: add, edit or delete a policy; forget what it learned"
  printf '  %sp protects an item, D deletes items older than 1, 3 or 5 days.%s\n' "$D" "$Z"
}

case "${1:-print}" in
  print) print ;;
  view)
    if command -v less >/dev/null; then
      print | less -R
    else
      print; read -r _
    fi ;;
  *) echo "usage: help.sh [print|view]" >&2; exit 2 ;;
esac
