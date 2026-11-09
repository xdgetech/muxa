#!/usr/bin/env bash
# shellcheck disable=SC2016  # jq programs: $ is jq's, not the shell's
# The Inbox of ADR 0033 in tmux: the sidebar's section, and prefix + T with
# the list above the selected item. muxa owns the items; this only shows them
# and answers them through muxa-cli (muxa.sh cli). Needs muxa's work level 3;
# Protect and Delete old need level 8 (ADR 0046).
# Usage: inbox.sh cache [--force] | rows | detail <target> | list <json> | view <id> <json>
#        inbox.sh key <id> <key> <json>  (what a key does: an action, choose:<n> or open:<n>)
#        inbox.sh ui <client> [item id]
set -u
. "$(dirname "$0")/lib.sh"

S="$AGENTS_SCRIPTS"
CACHE="$(agents_run_dir)/inbox.json"
# A lock older than this, in seconds, belongs to a refresh that died.
LOCK_LIMIT=60
# muxa's work level with protect and delete-inbox.
WORK_DELETE=8

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
  def marks: (if .laterAt or .state == "later" then " · later" else "" end) +
    (if .protectedAt then " · protected" else "" end);
  # p toggles Protect; only a host with work level 8 takes it (ADR 0046).
  def protect_act($w): if $w < 8 then empty elif .protectedAt then "unprotect" else "protect" end;
  # Labels name the result as the apps do (ADR 0033 §4a, ADR 0040): ✓ closes only
  # the item, and an answer that changes the Task names it. A proposal keeps
  # Approve and Reject.
  def answer_label($a):
    (.kind == "ask") as $ask |
    {"approve": (if $ask then "Allow" else "Approve" end), "accept": "Complete Task",
     "retry": "Retry", "reject": (if $ask then "Deny" else "Reject" end),
     "reply": (if $ask then "Deny & Send" else "Send" end),
     "keep-open": "✓ Done", "ask-agent": "Ask Agent",
     "drop": "Drop Task", "later": "Later", "dismiss": "✓ Done"}[$a] // $a;
  # [{key, act, group}] as the apps place them (ADR 0040): triage is the top bar
  # and its … menu, answers are the decisions and the reply box. `a` is never
  # Complete Task, which the apps keep in the menu.
  def keymap: (.answers // []) as $a |
    def pick($acts): [$a[] | select(IN($acts[]))][0] // empty;
    def bind($key; $act; $group): {key: $key, act: $act, group: $group};
    [(pick(["later"]) | bind("l"; .; "triage")),
     (pick(["keep-open", "dismiss"]) | bind("d"; .; "triage")),
     (pick(["accept"]) | bind("c"; .; "triage")),
     (pick(["drop"]) | bind("x"; .; "triage")),
     (pick(["approve", "retry"]) | bind("a"; .; "answer")),
     (pick(["reject"]) | bind("r"; .; "answer")),
     (pick(["reply", "ask-agent"]) | bind("s"; .; "answer"))];
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
    "\(.id)\t\(dot) \(icon) \(.question)  \u001b[2m\(.title) · \(age)\(marks)\u001b[0m"
  ' "$1"
}

# The preview of one item: question, summary, evidence, options, then the
# keys: triage on one line, answers on the next.
view() {
  iq -r --arg item "$1" "$JQ_DEFS"'
    (.work // 0) as $w | .items[] | select(.id == $item) | . as $i |
    "\(icon) \u001b[1m\(.question)\u001b[0m",
    "\u001b[2m\(.title) · \(.workspace) · \(age)\(if .readAt then "" else " · unread" end)\(if .protectedAt then " · protected" else "" end)\u001b[0m",
    "",
    (.summary // empty), (if .summary then "" else empty end),
    (evidence as $e | if ($e | length) > 0 then
      "\u001b[1mEvidence\u001b[0m", ($e | to_entries[] | "  \(.key + 1)  \(.value.label)"), ""
     else empty end),
    (if .prompt.kind == "question" and ((.answers // []) | index("choose")) then
      "\u001b[1m\(.prompt.question // "Options")\u001b[0m",
      (.prompt.options | to_entries[] | "  \(.key + 1)  \(.value.label)"), ""
     else empty end),
    ([keymap[] | select(.group == "triage") | . as $k | "\($k.key) \($i | answer_label($k.act))"] +
      [protect_act($w) | if . == "protect" then "p Protect" else "p Unprotect" end] +
      (if .session then ["o Open Session"] else [] end) | select(length > 0) | join("   ")),
    ([keymap[] | select(.group == "answer") | . as $k | "\($k.key) \($i | answer_label($k.act))"]
      | select(length > 0) | join("   ")),
    "\u001b[2mspace/b scroll   v read all   \(if $w >= 8 then "D delete old   " else "" end)q close\u001b[0m"
  ' "$2"
}

# What a key does for an item: an action, choose:<n>, open:<n>, or nothing.
key_action() {
  iq -r --arg item "$1" --arg key "$2" "$JQ_DEFS"'
    (.work // 0) as $w | .items[] | select(.id == $item) |
    if $key == "p" then protect_act($w) // ""
    elif ($key | test("^[1-9]$")) then
      if .prompt.kind == "question" and ((.answers // []) | index("choose"))
      then "choose:\($key | tonumber - 1)" else "open:\($key)" end
    else (keymap[] | select(.key == $key) | .act) // "" end
  ' "$3" | head -n 1
}

pause() { echo "Press Enter."; read -r _; }

# What answer() asks before an action's text.
text_prompt() {
  case "$1" in
    ask-agent) echo "Request for the workspace's agent, with this item (the item stays open):" ;;
    reply) if [ "$2" = ask ]; then echo "Deny, then tell the agent instead:"; else echo "Message for the agent:"; fi ;;
  esac
}

# Runs one answer of the item in <json>. Asks for the text an action needs.
answer() {
  local id="$1" act="$2" json="$3" text err kind why
  set -- inbox answer "$id"
  case "$act" in
    choose:*) set -- "$@" choose --option "${act#choose:}" ;;
    reply | ask-agent)
      clear
      kind="$(jq -r --arg item "$id" '.items[] | select(.id == $item) | .kind' "$json")"
      printf '%s\n\n' "$(text_prompt "$act" "$kind")"
      IFS= read -r -e -p '> ' text || return 0
      [ -n "$text" ] || return 0
      set -- "$@" "$act" --text "$text"
      ;;
    *) set -- "$@" "$act" ;;
  esac
  if ! err="$(muxa "$@" --json 2>&1 >/dev/null)"; then
    why="$(lost_work "$err")"
    if [ -n "$why" ]; then
      # An answer that ends the Task would lose work (ADR 0043).
      printf '\n'
      confirm_loss "$why" || return 0
      err="$(muxa "$@" --force --json 2>&1 >/dev/null)" && return 0
    fi
    printf '\nmuxa could not answer: %s\n' "$(printf '%s' "$err" | sed 's/^muxa-cli: //' | tail -n 1)"
    pause
  fi
}

# How many listed items delete_old removes: older than <days>, not protected.
old_count() {
  iq -r --argjson days "$2" \
    '[.items[] | select(.createdAt < ($now - $days * 86400) and (.protectedAt | not))] | length' "$1"
}

# D: deletes the items older than 1, 3 or 5 days, after a confirmation that
# names how many (ADR 0046). Protected items stay.
delete_old() {
  local json="$1" days ok n err
  clear
  if [ "$(jq -r '.work // 0' "$json")" -lt "$WORK_DELETE" ]; then
    echo "This host cannot delete Inbox items; update muxa-bridge on the host."
    pause
    return 0
  fi
  printf 'Delete Inbox items older than how many days? Protected items stay.\n\n'
  IFS= read -r -n 1 -p '1, 3 or 5 (any other key cancels): ' days || return 0
  case "$days" in 1 | 3 | 5) ;; *) return 0 ;; esac
  n="$(old_count "$json" "$days")"
  printf '\n\nDelete %s items older than %s days? [y/N] ' "$n" "$days"
  IFS= read -r -n 1 ok || return 0
  [ "$ok" = y ] || return 0
  if ! err="$(muxa inbox delete --older-than "$days" --json 2>&1 >/dev/null)"; then
    printf '\nmuxa could not delete: %s\n' "$(printf '%s' "$err" | sed 's/^muxa-cli: //' | tail -n 1)"
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
    # --sync so start:pos lands after the rows load, also after an answer.
    picked="$(printf '%s\n' "$rows" | fzf --sync --ansi --no-input --delimiter '\t' --with-nth 2 --no-sort \
      --layout reverse --info hidden --pointer '▌' --highlight-line \
      --preview "'$S/inbox.sh' view {1} '$json'" --preview-window 'down,70%,wrap,border-top' \
      --bind "start:pos(${pos:-1})" --bind 'j:down,k:up,q:abort' \
      --bind 'space:preview-page-down,b:preview-page-up,J:preview-down,K:preview-up' \
      --expect 'enter,ctrl-r,a,c,d,s,r,x,l,o,p,v,D,1,2,3,4,5,6,7,8,9')" || break
    key="$(printf '%s\n' "$picked" | sed -n '1p')"
    id="$(printf '%s\n' "$picked" | sed -n '2p' | cut -f1)"
    focus="$id"
    [ -n "$id" ] || continue
    case "$key" in
      ctrl-r) continue ;;
      v) read_item "$id" "$json" ;;
      D) delete_old "$json" ;;
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
