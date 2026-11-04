#!/usr/bin/env bash
# shellcheck disable=SC2016  # jq programs: $ is jq's, not the shell's
# The Inbox of ADR 0033 in tmux: the sidebar's section, and prefix + T with
# the list above the selected item. muxa owns the items; this only shows them
# and answers them through muxa-cli (muxa.sh cli). Needs muxa's work level 3.
# Usage: inbox.sh cache [--force] | rows | detail <target> | list <json> | view <id> <json>
#        inbox.sh key <id> <key> <json>  (what a key does: an action, choose:<n> or open:<n>)
#        inbox.sh ui <client> [item id]
set -u
. "$(dirname "$0")/lib.sh"

S="$AGENTS_SCRIPTS"
CACHE="$(agents_run_dir)/inbox.json"
# A lock older than this, in seconds, belongs to a refresh that died.
LOCK_LIMIT=60

muxa() { "$S/muxa.sh" cli "$@"; }

mtime() { stat -f %m "$1" 2>/dev/null || stat -c %Y "$1" 2>/dev/null || echo 0; }

# Refreshes the cache when it is older than @agents_inbox_refresh seconds, or
# with --force. One refresh runs at a time. A changed Inbox reloads the sidebars.
cache() {
  local every lock tmp changed=0
  "$S/muxa.sh" available || return 0
  every="$(opt @agents_inbox_refresh 5)"
  if [ "${1:-}" != --force ] && [ -f "$CACHE" ] &&
    [ $(($(date +%s) - $(mtime "$CACHE"))) -lt "$every" ]; then
    return 0
  fi
  lock="$CACHE.lock"
  if ! mkdir "$lock" 2>/dev/null; then
    [ $(($(date +%s) - $(mtime "$lock"))) -ge "$LOCK_LIMIT" ] || return 0
    rmdir "$lock" 2>/dev/null
    mkdir "$lock" 2>/dev/null || return 0
  fi
  tmp="$(mktemp "$CACHE.XXXXXX")" || { rmdir "$lock"; return 1; }
  if muxa inbox --json </dev/null >"$tmp" 2>/dev/null && jq -e . "$tmp" >/dev/null 2>&1; then
    if cmp -s "$tmp" "$CACHE"; then
      rm -f "$tmp"
      touch "$CACHE"
    else
      mv "$tmp" "$CACHE"
      changed=1
    fi
  else
    rm -f "$tmp"
    log "muxa could not list the Inbox; see muxa-cli inbox"
  fi
  rmdir "$lock" 2>/dev/null
  [ "$changed" = 1 ] && "$S/refresh.sh" </dev/null >/dev/null 2>&1 &
  return 0
}

# jq with the icons and what an item shows, shared by every view below.
iq() {
  local w e d
  IFS=$'\037' read -r w e d <<EOF
$(t display-message -p "#{@agents_icon_waiting}"$'\037'"#{@agents_icon_error}"$'\037'"#{@agents_icon_done}" 2>/dev/null)
EOF
  jq --arg iw "${w:-🙋}" --arg ie "${e:-❌}" --arg id "${d:-🏁}" --argjson now "$(date +%s)" "$@"
}

JQ_DEFS='
  def icon: {"ask": $iw, "failure": $ie, "review": $id, "proposal": "📝", "briefing": "📰"}[.kind] // "•";
  def age: ($now - .createdAt) as $s |
    if $s < 60 then "\($s)s" elif $s < 3600 then "\($s / 60 | floor)m"
    elif $s < 86400 then "\($s / 3600 | floor)h" else "\($s / 86400 | floor)d" end;
  def dot: if .readAt then " " else "●" end;
  def shown: select(.state == "open" or .state == "later");
  # Labels name the result (ADR 0033 §4a). A proposal keeps Approve and Reject.
  def answer_label($a):
    (.kind == "ask") as $ask |
    {"approve": (if $ask then "Allow" else "Approve" end), "accept": "Done (closes the Task)",
     "retry": "Retry", "reject": (if $ask then "Deny" else "Reject" end),
     "reply": (if $ask then "Deny & send" else "Send" end),
     "redo": "Redo", "change-course": "Change course", "keep-open": "Keep open",
     "drop": "Drop", "later": "Later",
     "dismiss": (if .kind == "briefing" then "Done" else "Dismiss" end)}[$a] // $a;
  # [{key, act}] in the order of .answers: a is the main answer, the rest have a key each.
  def keymap: (.answers // []) as $a | [
    (if ($a | length) > 0 and ($a[0] | IN("choose", "later") | not) then {key: "a", act: $a[0]} else empty end),
    (if $a | index("reply") then {key: "s", act: "reply"} else empty end),
    (if $a | index("reject") then {key: "r", act: "reject"}
     elif $a | index("redo") then {key: "r", act: "redo"} else empty end),
    (if $a | index("change-course") then {key: "c", act: "change-course"} else empty end),
    (if $a | index("keep-open") then {key: "p", act: "keep-open"} else empty end),
    (if $a | index("drop") then {key: "x", act: "drop"} else empty end),
    (if ($a | index("dismiss")) and $a[0] != "dismiss" then {key: "n", act: "dismiss"} else empty end),
    (if $a | index("later") then {key: "l", act: "later"} else empty end)] | sort_by(.act as $x | $a | index($x));
  # Numbered evidence: the changes, docs, images and links.
  def evidence: (.attachments // {}) as $t | [
    ($t.changes // empty | {kind: "diff", ref: (.scope // ""),
      label: "Changes: \(.files) files, +\(.added) −\(.removed)\(if .scope == "branch" then " (branch)" else "" end)"}),
    ($t.docs // [] | .[] | {kind: "doc", ref: ., label: .}),
    ($t.images // [] | .[] | {kind: "image", ref: ., label: .}),
    ($t.links // [] | .[] | {kind: "link", ref: .url, label: ((.title // .url) + "  " + .url)})];
'

# The sidebar's section: a heading row, then the first open items.
rows() {
  [ -s "$CACHE" ] || return 0
  "$S/muxa.sh" available || return 0
  iq -r --argjson max "$(opt @agents_inbox_rows 3)" "$JQ_DEFS"'
    select((.work // 0) >= 3) | [.items[] | select(.state == "open")] as $open |
    "I:\t\u001b[1mInbox\u001b[0m" + (if ($open | length) > 0 then " \($open | length)" else "\u001b[2m · nothing needs you\u001b[0m" end),
    ($open[:$max][] | "i:\(.id)\t \(dot) \(icon) \(.question)")
  ' "$CACHE"
}

# The sidebar footer for an Inbox row.
detail() {
  local target="$1"
  case "$target" in
    I:) printf '\033[1mInbox\033[0m\n\033[2mEnter open  prefix + T\033[0m\n' ;;
    i:*)
      [ -s "$CACHE" ] || return 0
      iq -r --arg item "${target#i:}" "$JQ_DEFS"'
        .items[] | select(.id == $item) |
        "\(icon) \u001b[1m\(.question)\u001b[0m", "\u001b[2m\(.title) · \(age)\u001b[0m",
        "\u001b[2mEnter open\u001b[0m"' "$CACHE"
      ;;
  esac
}

# prefix + T: one row per open or later item: <id> TAB <display>.
list() {
  iq -r "$JQ_DEFS"'
    .items[] | shown |
    "\(.id)\t\(dot) \(icon) \(.question)  \u001b[2m\(.title) · \(age)\(if .state == "later" then " · later" else "" end)\u001b[0m"
  ' "$1"
}

# The preview of one item: question, summary, evidence, options and keys.
view() {
  iq -r --arg item "$1" "$JQ_DEFS"'
    .items[] | select(.id == $item) | . as $i |
    "\(icon) \u001b[1m\(.question)\u001b[0m",
    "\u001b[2m\(.title) · \(.workspace) · \(age)\(if .readAt then "" else " · unread" end)\u001b[0m",
    "",
    (.summary // empty), (if .summary then "" else empty end),
    (evidence as $e | if ($e | length) > 0 then
      "\u001b[1mEvidence\u001b[0m", ($e | to_entries[] | "  \(.key + 1)  \(.value.label)"), ""
     else empty end),
    (if .prompt.kind == "question" and ((.answers // []) | index("choose")) then
      "\u001b[1m\(.prompt.question // "Options")\u001b[0m",
      (.prompt.options | to_entries[] | "  \(.key + 1)  \(.value.label)"), ""
     else empty end),
    ([keymap[] | . as $k | "\($k.key) \($i | answer_label($k.act))"] +
      (if .session then ["o open session"] else [] end) + ["space/b scroll", "v read all", "q close"] | join("   "))
  ' "$2"
}

# What a key does for an item: an action, choose:<n>, open:<n>, or nothing.
key_action() {
  iq -r --arg item "$1" --arg key "$2" "$JQ_DEFS"'
    .items[] | select(.id == $item) |
    if ($key | test("^[1-9]$")) then
      if .prompt.kind == "question" and ((.answers // []) | index("choose"))
      then "choose:\($key | tonumber - 1)" else "open:\($key)" end
    else (keymap[] | select(.key == $key) | .act) // "" end
  ' "$3" | head -n 1
}

pause() { echo "Press Enter."; read -r _; }

# What answer() asks before an action's text.
text_prompt() {
  case "$1" in
    redo) echo "Feedback for the agent's last turn:" ;;
    change-course) echo "New direction for the agent:" ;;
    reply) if [ "$2" = ask ]; then echo "Deny, then tell the agent instead:"; else echo "Message for the agent:"; fi ;;
  esac
}

# Runs one answer of the item in <json>. Asks for the text or the time an action needs.
answer() {
  local id="$1" act="$2" json="$3" text until err kind
  set -- inbox answer "$id"
  case "$act" in
    choose:*) set -- "$@" choose --option "${act#choose:}" ;;
    reply | redo | change-course)
      clear
      kind="$(jq -r --arg item "$id" '.items[] | select(.id == $item) | .kind' "$json")"
      printf '%s\n\n' "$(text_prompt "$act" "$kind")"
      IFS= read -r -e -p '> ' text || return 0
      [ -n "$text" ] || return 0
      set -- "$@" "$act" --text "$text"
      ;;
    later)
      IFS= read -r -e -p 'Later until (+30m, +1h, +1d or 2026-11-05T09:00Z) [+1h]: ' until || return 0
      set -- "$@" later --until "${until:-+1h}"
      ;;
    *) set -- "$@" "$act" ;;
  esac
  if ! err="$(muxa "$@" --json 2>&1 >/dev/null)"; then
    printf '\nmuxa could not answer: %s\n' "$(printf '%s' "$err" | sed 's/^muxa-cli: //' | tail -n 1)"
    pause
  fi
}

# The item's pane directory and repo root, for its evidence.
item_dir() {
  local pane
  pane="$(jq -r --arg item "$1" '.items[] | select(.id == $item) | .pane // empty' "$2")"
  [ -n "$pane" ] || return 1
  t display-message -p -t "$pane" '#{pane_current_path}' 2>/dev/null
}

# The whole item in the pager, for a screen too small for the preview.
# shellcheck disable=SC2086  # $pager is a command with its flags
read_item() {
  local pager="${PAGER:-less -R}"
  view "$1" "$2" | $pager
  muxa inbox read "$1" </dev/null >/dev/null 2>&1 &
}

# Opens evidence <n> of an item: the diff or a doc in the pager, an image or
# a link with the system opener when there is one, else its path or URL.
# shellcheck disable=SC2086  # $pager is a command with its flags
open_evidence() {
  local id="$1" n="$2" json="$3" kind ref dir root base pager="${PAGER:-less -R}"
  IFS=$'\t' read -r kind ref <<EOF
$(iq -r --arg item "$id" --argjson n "$n" "$JQ_DEFS"'
  .items[] | select(.id == $item) | evidence[$n - 1] // empty | "\(.kind)\t\(.ref)"' "$json")
EOF
  [ -n "$kind" ] || return 0
  muxa inbox read "$id" </dev/null >/dev/null 2>&1 &
  if [ "$kind" = link ]; then
    t set-buffer -- "$ref"
    if command -v open >/dev/null && [ -z "${SSH_CONNECTION:-}" ]; then open "$ref"; else
      printf '%s\n(copied to the tmux buffer)\n' "$ref"; pause; fi
    return 0
  fi
  dir="$(item_dir "$id" "$json")" || { echo "The item's pane is gone."; pause; return 0; }
  root="$(git -C "$dir" rev-parse --show-toplevel 2>/dev/null)" || root="$dir"
  case "$kind" in
    diff)
      if [ "$ref" = branch ]; then
        base="$(git -C "$root" merge-base HEAD "$(default_base "$root")" 2>/dev/null)"
        git -C "$root" -c color.ui=always diff "${base:-HEAD}" HEAD | $pager
      else
        git -C "$root" -c color.ui=always diff HEAD | $pager
      fi
      ;;
    doc) $pager "$root/$ref" ;;
    image)
      if command -v open >/dev/null && [ -z "${SSH_CONNECTION:-}" ]; then open "$root/$ref"; else
        printf '%s\n' "$root/$ref"; pause; fi
      ;;
  esac
}

ui() {
  local client="$1" focus="${2:-}" json rows pos picked key id act sid
  json="$(agents_run_dir)/inbox-ui-$$.json"
  trap 'rm -f "$json"' EXIT
  while :; do
    if ! muxa inbox --json </dev/null >"$json" 2>/dev/null; then
      echo "muxa could not list the Inbox; try muxa-cli inbox."
      pause
      return 1
    fi
    # An older muxa has no items: its Task list instead.
    if [ "$(jq -r '.work // 0' "$json")" -lt 3 ]; then
      exec "$S/muxa.sh" inbox-ui "$client"
    fi
    rows="$(list "$json")"
    if [ -z "$rows" ]; then
      echo "Nothing needs you."
      sleep 1.5
      break
    fi
    pos="$(printf '%s\n' "$rows" | awk -F'\t' -v f="$focus" '$1 == f { print NR; exit }')"
    picked="$(printf '%s\n' "$rows" | fzf --ansi --no-input --delimiter '\t' --with-nth 2 --no-sort \
      --layout reverse --info hidden --pointer '▌' --highlight-line \
      --preview "'$S/inbox.sh' view {1} '$json'" --preview-window 'down,70%,wrap,border-top' \
      --bind "start:pos(${pos:-1})" --bind 'j:down,k:up,q:abort' \
      --bind 'space:preview-page-down,b:preview-page-up,J:preview-down,K:preview-up' \
      --expect 'enter,ctrl-r,a,s,r,c,p,x,n,l,o,v,1,2,3,4,5,6,7,8,9')" || break
    key="$(printf '%s\n' "$picked" | sed -n '1p')"
    id="$(printf '%s\n' "$picked" | sed -n '2p' | cut -f1)"
    focus="$id"
    [ -n "$id" ] || continue
    case "$key" in
      ctrl-r) continue ;;
      v) read_item "$id" "$json" ;;
      enter | o)
        sid="$(jq -r --arg item "$id" '.items[] | select(.id == $item) | .session // empty' "$json")"
        if [ -n "$sid" ] && t has-session -t "$sid" 2>/dev/null; then
          muxa inbox read "$id" </dev/null >/dev/null 2>&1
          "$S/muxa.sh" goto "$client" "$sid"
          break
        fi
        echo "This item has no session to open."
        sleep 1.5
        ;;
      *)
        act="$(key_action "$id" "$key" "$json")"
        case "$act" in
          "") ;;
          open:*) open_evidence "$id" "${act#open:}" "$json" ;;
          *) answer "$id" "$act" "$json" ;;
        esac
        ;;
    esac
  done
  cache --force
}

case "${1:-}" in
  cache) cache "${2:-}" ;;
  rows) rows ;;
  detail) detail "${2:?target}" ;;
  list) list "${2:?json}" ;;
  view) view "${2:?item id}" "${3:?json}" ;;
  key) key_action "${2:?item id}" "${3:?key}" "${4:?json}" ;;
  ui) ui "${2:?client}" "${3:-}" ;;
  *) echo "usage: inbox.sh cache [--force] | rows | detail <target> | list <json> | view <id> <json> | key <id> <key> <json> | ui <client> [item id]" >&2; exit 2 ;;
esac
