#!/usr/bin/env bash
# shellcheck disable=SC2016  # jq programs: $ is jq's, not the shell's
# The Policies list of ADR 0050 in tmux, opened with y in the Inbox: the
# policies (y-), the overall assistant's hypotheses (h-) and the recent
# corrections (x-), from muxa's work level 11. Changes go through muxa-cli.
# Usage: policies.sh list <json> | view <id> <json> | key <id> <key> <json> | ui
set -u
. "$(dirname "$0")/lib.sh"

S="$AGENTS_SCRIPTS"

muxa() { "$S/muxa.sh" cli "$@"; }

DEFS='
  def plain: gsub("[\\x00-\\x08\\x0b-\\x1f\\x7f]"; "");
  def one_line: plain | gsub("[\n\t]"; " ");
  def day: strftime("%Y-%m-%d %H:%MZ");
  def expired: .until != null and .until <= $now;
  def span: (if .from then " · from \(.from | day)" else "" end) +
    (if .until then " · until \(.until | day)" else " · no end" end);
  def evidence: (.evidence // []) | length | "based on \(.) correction\(if . == 1 then "" else "s" end)";
  # The open Inbox item that asks about hypothesis $h, for Confirm and Reject.
  def item_of($root; $h): [($root.items // [])[] | select(.kind == "hypothesis" and .state == "open"
    and .hypothesis.hypothesis == $h) | .id][0];
  def correction_word: {"reordered": "reordered Today", "dismissed": "not today",
    "rejected": "rejected", "pinned": "pinned", "unpinned": "unpinned", "deferred": "later"}[.kind] // "unknown";
'

pq() { jq --argjson now "$(date +%s)" "$@"; }

# <id> TAB <display>: Add first, then each section with its heading.
list() {
  pq -r "$DEFS"'
    . as $root |
    "add:\t＋ Add a policy",
    (if ((.policies // []) | length) > 0 then "P:\t\u001b[1mPolicies\u001b[0m" else empty end),
    ((.policies // [])[] |
      "\(.id)\t  \(if expired then "\u001b[2m" else "" end)📌 \(.text | one_line)  \u001b[2m\(.scope | plain)\(span)\(if expired then " · expired" else "" end)\u001b[0m"),
    (if ((.hypotheses // []) | length) > 0 then "H:\t\u001b[1mHypotheses\u001b[0m" else empty end),
    ((.hypotheses // [])[] |
      "\(.id)\t  💡 \(.text | one_line)  \u001b[2m\(.state) · \(.confidence) · \(.scope | plain)\u001b[0m"),
    (if ((.corrections // []) | length) > 0 then "X:\t\u001b[1mRecent corrections\u001b[0m" else empty end),
    ((.corrections // [])[] |
      "\(.id)\t  ↺ \(correction_word)\(if .row then " \(.row)" else "" end)  \u001b[2m\(.at | day)\(if .reason then " · \(.reason | one_line)" else "" end)\u001b[0m")
  ' "$1"
}

view() {
  pq -r --arg id "$1" "$DEFS"'
    . as $root |
    if $id == "add:" then
      "＋ \u001b[1mAdd a policy\u001b[0m", "",
      "A policy is your own words, one paragraph. It outranks every hypothesis",
      "and note of the overall assistant, for all workspaces or one, until its end.",
      "", "n Add"
    elif $id == "P:" or $id == "H:" or $id == "X:" then
      "What the overall assistant remembers. Each plan cites what it used.", "", "n Add a policy"
    else
      ((.policies // [])[] | select(.id == $id) |
        "📌 \u001b[1m\(.text | plain)\u001b[0m",
        "\u001b[2m\(.id) · \(.scope | plain)\(span)\(if expired then " · expired" else "" end)\(if .source != "user" then " · confirmed from \(.source)" else "" end)\u001b[0m",
        "", "e Edit   d Delete"),
      ((.hypotheses // [])[] | select(.id == $id) | item_of($root; .id) as $item |
        "💡 \u001b[1m\(.text | plain)\u001b[0m",
        "\u001b[2m\(.id) · \(.state) · confidence \(.confidence) · \(.scope | plain)\u001b[0m",
        "", "The overall assistant guessed this, \(evidence): \((.evidence // []) | join(" "))",
        "Plans made before you forget it keep their text.",
        "", (if $item then "a Confirm   r Reject   " else "" end) + "f Forget"),
      ((.corrections // [])[] | select(.id == $id) |
        "↺ \u001b[1m\(correction_word)\(if .row then " \(.row)" else "" end)\u001b[0m",
        "\u001b[2m\(.id) · \(.plan) · \(.at | day)\(if .inTop then "" else " · not in the plan" end)\u001b[0m",
        (if (.after // []) | length > 0 then "Order: \(.after | join(" "))" else empty end),
        (if .reason then "", "Reason: \(.reason | plain)" else empty end),
        "", "Forgetting it also removes the hypotheses it grounds.",
        "Plans made before keep their text.", "", "f Forget")
    end,
    "\u001b[2mq back to the Inbox\u001b[0m"
  ' "$2"
}

# What a key does on a row: add, edit, delete, confirm:<item>, reject:<item>,
# forget, or nothing.
key_action() {
  [ "$2" = n ] && { echo add; return; }
  pq -r --arg id "$1" --arg key "$2" "$DEFS"'
    if ($id | startswith("y-")) then
      {"e": "edit", "d": "delete"}[$key] // empty
    elif ($id | startswith("h-")) then
      item_of(.; $id) as $item |
      if $key == "f" then "forget"
      elif $key == "a" and $item then "confirm:\($item)"
      elif $key == "r" and $item then "reject:\($item)"
      else empty end
    elif ($id | startswith("x-")) and $key == "f" then "forget"
    else empty end
  ' "$3"
}

pause() { echo "Press Enter."; read -r _; }

yes_no() {
  local reply
  IFS= read -r -e -p "$1 [y/N] > " reply || return 1
  case "$reply" in y | Y | yes) return 0 ;; *) return 1 ;; esac
}

# Adds the scope and end flags the user gives. $1 is what Enter keeps.
span_flags() {
  local scope until
  IFS= read -r -e -p "Scope: all or a workspace (Enter: $1) > " scope || return 1
  IFS= read -r -e -p "Until: w this week, YYYY-MM-DD HH:MM (Enter: $2) > " until || return 1
  [ -z "$scope" ] || printf -- '--scope\n%s\n' "$scope"
  case "$until" in
    "") ;;
    w) printf -- '--until\nweek\n' ;;
    *) printf -- '--until\n%s\n' "$until" ;;
  esac
}

act() {
  local id="$1" act="$2" text line flags
  case "$act" in
    add)
      clear
      printf 'A policy in your own words, one paragraph.\n\n'
      IFS= read -r -e -p '> ' text || return 0
      [ -n "$text" ] || return 0
      flags="$(span_flags all "no end")" || return 0
      set -- policy add "$text"
      ;;
    edit)
      clear
      printf 'Edit %s. Enter keeps a field.\n\n' "$id"
      IFS= read -r -e -p 'Text > ' text || return 0
      flags="$(span_flags keep keep)" || return 0
      set -- policy edit "$id"
      [ -z "$text" ] || set -- "$@" --text "$text"
      ;;
    delete)
      clear
      yes_no "Delete $id? The plans that cite it keep their text." || return 0
      set -- policy delete "$id"
      ;;
    forget)
      clear
      yes_no "Forget $id? The next plans cannot use it." || return 0
      set -- forget "$id"
      ;;
    confirm:*) set -- inbox answer "${act#confirm:}" confirm ;;
    reject:*)
      clear
      printf 'Reject. A reason helps the next plan; it is never required.\n\n'
      IFS= read -r -e -p 'Reason (optional, Enter to skip) > ' text || return 0
      set -- inbox answer "${act#reject:}" reject
      [ -z "$text" ] || set -- "$@" --reason "$text"
      ;;
    *) return 0 ;;
  esac
  while IFS= read -r line; do
    [ -n "$line" ] && set -- "$@" "$line"
  done <<EOF
${flags:-}
EOF
  if [ "$act" = edit ] && [ $# -eq 3 ]; then
    return 0
  fi
  if ! text="$(muxa "$@" --json 2>&1 >/dev/null)"; then
    printf '\nmuxa could not save it: %s\n' "$(printf '%s' "$text" | sed 's/^muxa-cli: //' | tail -n 1)"
    pause
  fi
}

ui() {
  local json rows pos picked key id act focus=""
  json="$(agents_run_dir)/policies-ui-$$.json"
  trap 'rm -f "$json"' EXIT
  while :; do
    if ! muxa inbox --json </dev/null >"$json" 2>/dev/null; then
      echo "muxa could not list the policies; try muxa-cli policy."
      pause
      return 1
    fi
    rows="$(list "$json")"
    pos="$(printf '%s\n' "$rows" | awk -F'\t' -v f="$focus" '$1 == f { print NR; exit }')"
    picked="$(printf '%s\n' "$rows" | fzf --sync --ansi --no-input --delimiter '\t' --with-nth 2 --no-sort \
      --layout reverse --info hidden --pointer '▌' --highlight-line \
      --preview "'$S/policies.sh' view {1} '$json'" --preview-window 'down,50%,wrap,border-top' \
      --bind "start:pos(${pos:-1})" --bind 'j:down,k:up,q:abort' \
      --expect 'enter,n,e,d,a,r,f')" || break
    key="$(printf '%s\n' "$picked" | sed -n '1p')"
    id="$(printf '%s\n' "$picked" | sed -n '2p' | cut -f1)"
    focus="$id"
    [ "$id" = add: ] && [ "$key" = enter ] && key=n
    act="$(key_action "$id" "$key" "$json")"
    [ -n "$act" ] && act "$id" "$act"
  done
}

case "${1:-}" in
  list) list "${2:?json}" ;;
  view) view "${2:?row id}" "${3:?json}" ;;
  key) key_action "${2:?row id}" "${3:?key}" "${4:?json}" ;;
  ui) ui ;;
  *) echo "usage: policies.sh list <json> | view <id> <json> | key <id> <key> <json> | ui" >&2; exit 2 ;;
esac
