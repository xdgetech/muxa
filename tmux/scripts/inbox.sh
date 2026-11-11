#!/usr/bin/env bash
# shellcheck disable=SC2016  # jq programs: $ is jq's, not the shell's
# The Inbox of ADR 0033 in tmux: the sidebar's section, and prefix + T with
# the list above the selected item. muxa owns the items; this only shows them
# and answers them through muxa-cli (muxa.sh cli). Needs muxa's work level 3;
# Protect and Delete old need level 8 (ADR 0046).
# With level 9, Work items (w-) and unsorted Captures (c-) follow the items;
# `a` on a Work item asks its workspace's assistant. With level 10, Today rows
# (today:) of the newest plan come first (ADR 0049); `t` asks for a plan.
# With level 11 (ADR 0050), K/J move a Today row, x takes it off Today, y
# opens the Policies list (policies.sh), and hypothesis items take Confirm.
# With level 12 (ADR 0051), `a` on a Work item asks which staff member takes
# it (Enter: the lead), a Work item shows its request's steps, and handoff
# items take Approve and Reject as the other proposals do.
# With level 13 (ADR 0052), write items show their target and content whole;
# Approve asks once more with the target, and Reject takes a reason.
# With level 14 (ADR 0053), memory items show a Task's text for the workspace
# memory whole; Approve asks once more with the workspace.
# Usage: inbox.sh cache [--force] | rows | detail <target> | list <json> | view <id> <json>
#        inbox.sh target <today row> <json>  (the item a Today row opens)
#        inbox.sh key <id> <key> <json>  (what a key does: an action, choose:<n> or open:<n>)
#        inbox.sh order <today row> up|down <json>  (the row ids plan reorder sends)
#        inbox.sh worker <work id> <json>  (asks on stdin which staff member; prints the name)
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
  def icon: {"ask": $iw, "failure": $ie, "review": $id, "proposal": "📝", "briefing": "📰",
    "sort": "🗂", "delegate": "🤝", "hypothesis": "💡", "handoff": "👥",
    "write": "📤", "memory": "🧠"}[.kind] // "•";
  def since($t): ($now - $t) as $s |
    if $s < 60 then "\($s)s" elif $s < 3600 then "\($s / 60 | floor)m"
    elif $s < 86400 then "\($s / 3600 | floor)h" else "\($s / 86400 | floor)d" end;
  def age: since(.createdAt);
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
     "drop": "Drop Task", "later": "Later", "dismiss": "✓ Done", "confirm": "Confirm"}[$a] // $a;
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
     (pick(["approve", "retry", "confirm"]) | bind("a"; .; "answer")),
     (pick(["reject"]) | bind("r"; .; "answer")),
     (pick(["reply", "ask-agent"]) | bind("s"; .; "answer"))];
  # Text the user or an agent wrote, without terminal control characters.
  def plain: gsub("[\\x00-\\x08\\x0b-\\x1f\\x7f]"; "");
  def work_word: {"in-progress": "in progress", "waiting-decision": "waiting on decision",
    "done": "done", "withdrawn": "withdrawn"}[.state] // "open";
  def open_work: (.workItems // [])[] | select(.state != "done" and .state != "withdrawn");
  # The newest request of an item, from the requests[] of the list.
  def asked($reqs): (.requests // [])[-1] as $q | [$reqs[] | select(.id == $q)][0] // empty;
  def delivery_word: if (.delivery | IN("pending", "received", "processing", "unconfirmed",
    "resulted", "failed", "timed-out", "cancel-requested", "cancelled"))
    then .delivery | gsub("-"; " ") else "unknown" end;
  def pin_due: (if .pinnedAt then " · pinned" else "" end) +
    (if .due then " · due \(.due.at | strftime("%Y-%m-%d %H:%MZ"))" else "" end);
  # Today (ADR 0049 §5): the newest plan says how planning went; the rows come
  # from the newest plan with a result.
  def latest_plan: (.plans // [])[0] // empty;
  def shown_plan: [(.plans // [])[] | select(.delivery == "resulted")][0] // empty;
  def has_today: (.work // 0) >= 10 and (((.plans // []) | length) > 0 or .assistant.overall != null);
  # Today in the order the user chose, when there is one (ADR 0050):
  # userOrder, then any row it misses. `top` keeps the order of the plan.
  def today_rows: (.top // []) as $top |
    [(.userOrder // [])[] as $id | $top[] | select((.work // .item) == $id)] as $u |
    $u + [$top[] | select(IN($u[]) | not)];
  def row_id: .work // .item;
  def not_today($p): row_id as $id | ($p.dismissed // []) | index($id) != null;
  # A basis that cites what the user told the assistant shows the text of
  # the record, or "forgotten" when the record is gone.
  def cited($root): .source as $s |
    if .kind == "policy" then [($root.policies // [])[] | select(.id == $s) | .text][0]
    elif .kind == "hypothesis" then [($root.hypotheses // [])[] | select(.id == $s) | .text][0]
    elif .kind == "correction" then [($root.corrections // [])[] | select(.id == $s) |
      "\(.kind)\(if .row then " \(.row)" else "" end)"][0]
    else .text end // "forgotten";
  def title_of($root; $id): ([($root.workItems // [])[], ($root.items // [])[] | select(.id == $id) | .title][0] // $id) | plain;
  def mins: "\(.minutes.low)–\(.minutes.high) min";
  def mark($p): (.work // .item) as $t |
    {"entered": " · new", "moved": " · moved"}[[($p.changed // [])[] | select(.id == $t) | .change][0] // ""] // "";
  def plan_status: if ((.plans // []) | length) == 0 then "no plan yet · t Plan Now" else latest_plan |
    if .delivery == "resulted" then "planned \(since(.at)) ago"
    elif (.delivery | IN("pending", "received", "processing")) then "planning…"
    else "last plan \(delivery_word)" end end;
  def first_line: [.text | plain | split("\n")[] | select(test("\\S"))][0] // "" | sub("^\\s+"; "") | .[:60];
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

# prefix + T: one row per open or later item, then the open Work items and
# the unsorted Captures (ADR 0048): <id> TAB <display>.
list() {
  iq -r "$JQ_DEFS"'
    . as $root | (.requests // []) as $reqs |
    (select(has_today) |
      "today:\t☀ \u001b[1mToday\u001b[0m  \u001b[2m\(plan_status)\u001b[0m",
      (shown_plan | . as $p | today_rows | to_entries[] | .value as $r |
        if $r | not_today($p) then
          "today:\(.key + 1)\t  \u001b[2m\(.key + 1). \(title_of($root; $r | row_id)) · not today\u001b[0m"
        else
          "today:\(.key + 1)\t  \(.key + 1). \(title_of($root; $r | row_id))  \u001b[2m\($r | mins)\($r | mark($p))\u001b[0m"
        end),
      (shown_plan | select((.dueSoon // []) | length > 0) |
        "today:due\t  ⏰ Due soon, moved down by you: \([.dueSoon[] as $d | title_of($root; $d)] | join(", "))")),
    (.items[] | shown |
    "\(.id)\t\(dot) \(icon) \(.question)  \u001b[2m\(.title) · \(age)\(marks)\u001b[0m"),
    (open_work |
    "\(.id)\t  ◆ \(.title | plain)  \u001b[2m\(work_word)\([asked($reqs) | " · asked: \(delivery_word)"][0] // "") · \(age)\(if .laterAt then " · later" else "" end)\(pin_due)\(if .workspace then " · \(.workspace)" else "" end)\u001b[0m"),
    ((.captures // [])[] | "\(.id)\t  ✎ \(first_line)  \u001b[2munsorted · \(age)\u001b[0m")
  ' "$1"
}

# A Work item: its title, state, links, its newest request and Result
# summary, and its keys.
view_work() {
  iq -r --arg item "$1" "$JQ_DEFS"'
    ((.work // 0) >= 10) as $pins | (.requests // []) as $reqs |
    .workItems[]? | select(.id == $item) |
    (.requests // []) as $mine |
    ([$reqs[] | select(.id | IN($mine[])) | .results[0]? // empty] | max_by(.at)) as $result |
    "◆ \u001b[1m\(.title | plain)\u001b[0m",
    "\u001b[2m\(work_word)\(if .workspace then " · \(.workspace)" else "" end) · \(age)\(if .laterAt then " · later" else "" end)\(pin_due)\u001b[0m",
    "",
    "Captures: \((.captures // []) | length)   Tasks: \((.tasks // []) | length)",
    (if .state == "waiting-decision" then "A Task waits for your decision above." else empty end),
    (asked($reqs) | "Assistant: \(.scope) v\(.version) · \(delivery_word) · \(.workspace | plain)",
      ((.steps // [])[] | "  \(.step). \(.worker | plain) · \(delivery_word)\(if .handoff then " → \(.handoff.worker | plain)" else "" end)")),
    (if $result then
      "", "\u001b[1mResult v\($result.version)\(if $result.stale then " (stale)" else "" end)\u001b[0m",
      ($result.summary | plain), "\u001b[2mAll of it: muxa-cli assist show \($item)\u001b[0m"
     else empty end),
    "",
    "l Later   d Complete   a Ask Assistant\(if $pins then "   P \(if .pinnedAt then "Unpin" else "Pin" end)" else "" end)",
    "\u001b[2mq close\u001b[0m"
  ' "$2"
}

# A Today row: the plan's reason, next step, minutes and basis; the heading:
# how planning went; the Due soon line: the items moved down that are due.
view_today() {
  iq -r --arg row "${1#today:}" "$JQ_DEFS"'
    . as $root | ((.work // 0) >= 11) as $memory |
    if $row == "" then
      "☀ \u001b[1mToday\u001b[0m", "\u001b[2m\(plan_status)\u001b[0m", "",
      (latest_plan | select(.delivery != "resulted") |
        "\(.id) · \(delivery_word)\(if .minutes then " · \(.minutes) min" else "" end)",
        (.reason // empty | plain), ""),
      (shown_plan |
        "\(.id) · \((.workspaces // []) | join(", "))",
        (if (.unanswered // []) | length > 0 then "Unanswered requests: \(.unanswered | join(" "))" else empty end),
        (if (.stale // []) | length > 0 then "Stale results: \(.stale | join(" "))" else empty end),
        ([(.changed // [])[] | select(.change == "left") | .id] | select(length > 0) |
          "Left the plan: \(join(" "))"),
        (if (.top // []) | length == 0 then "Nothing to rank." else empty end), ""),
      "t Plan Now\(if $memory then "   y Policies" else "" end)"
    elif $row == "due" then
      shown_plan |
      "⏰ \u001b[1mDue soon, moved down by you\u001b[0m", "",
      (.dueSoon[] as $d | ([($root.workItems // [])[] | select(.id == $d)][0] // {id: $d, title: $d}) |
        "◆ \(.title | plain)\(pin_due)"),
      "", "o Open   t Plan Now"
    else
      shown_plan | . as $p | today_rows[$row | tonumber - 1] // empty |
      "\($row). \u001b[1m\(title_of($root; row_id))\u001b[0m",
      "\u001b[2m\(row_id)\(mark($p))\(if not_today($p) then " · not today" else "" end)\u001b[0m", "",
      "Why: \(.why | plain)", "Next: \(.next | plain)",
      "Time: \(mins), about \(.minutes.estimated)",
      (if (.basis // []) | length > 0 then "", "\u001b[1mBasis\u001b[0m",
        (.basis[] | "  \(.kind): \(cited($root) | plain)\(if .source then "  \(.source | plain)" else "" end)")
       else empty end),
      (if (.missing // []) | length > 0 then "", "\u001b[1mWould help\u001b[0m", (.missing[] | "  \(plain)")
       else empty end),
      "", (if $memory then "K/J Move   " + (if not_today($p) then "" else "x Not today   " end) else "" end) +
        "o Open   t Plan Now"
    end,
    "\u001b[2mq close\u001b[0m"
  ' "$2"
}

# An unsorted Capture: the whole text, and what sorting it does.
view_capture() {
  iq -r --arg item "$1" "$JQ_DEFS"'
    .captures[]? | select(.id == $item) |
    "✎ \u001b[1mUnsorted\u001b[0m",
    "\u001b[2m\(age)\(if .workspace then " · \(.workspace)" else "" end)\u001b[0m",
    "",
    (.text | plain),
    "",
    "w New Work item   n Keep as note",
    "\u001b[2mspace/b scroll   q close\u001b[0m"
  ' "$2"
}

# The preview of one item: question, summary, evidence, options, then the
# keys: triage on one line, answers on the next.
view() {
  case "$1" in
    w-*) view_work "$@"; return ;;
    c-*) view_capture "$@"; return ;;
    today:*) view_today "$@"; return ;;
  esac
  iq -r --arg item "$1" "$JQ_DEFS"'
    (.work // 0) as $w | (.requests // []) as $reqs | .items[] | select(.id == $item) | . as $i |
    "\(icon) \u001b[1m\(.question)\u001b[0m",
    "\u001b[2m\(.title) · \(.workspace) · \(age)\(if .readAt then "" else " · unread" end)\(if .protectedAt then " · protected" else "" end)\u001b[0m",
    "",
    (.summary // empty), (if .summary then "" else empty end),
    (.delegate // empty |
      "To: \(.workspace | plain)\(if .worker then " · \(.worker | plain)" else "" end) · \(.scope)", "Ask: \(.purpose | plain)", "Why: \(.reason | plain)", ""),
    (.handoff // empty |
      "From: \(.from | plain) → \(.worker | plain) · step \(.step + 1)", "Ask: \(.purpose | plain)",
      "Approve starts the next step; Reject ends the request with what it has.", ""),
    (.write // empty | ([$reqs[] | select(.id == $i.write.request) | .version][0] // .version) as $current |
      "Action: \(.action | plain)", "To: \(.target | plain)",
      "From: \(.worker // "the assistant" | plain) · v\(.version)\(if .version < $current then " · from an older result" else "" end)",
      "", "Content, written exactly as below:", (.content | plain), "",
      "Approve starts one Task of the writer with this text; muxa does not check the result.", ""),
    (.memory // empty |
      "From: Task \(.task)",
      "", "Added to the workspace memory exactly as below:", (.text | plain), "",
      "Approve appends it to .muxa/WORKSPACE.md under the Task'"'"'s title; every later run reads it.", ""),
    (.hypothesis // empty |
      "Scope: \(.scope | plain) · confidence \(.confidence)",
      "Based on \((.evidence // []) | length) correction\(if ((.evidence // []) | length) == 1 then "" else "s" end): \((.evidence // []) | join(" "))",
      "Confirm keeps it as a policy in these words.", ""),
    (.sort // empty |
      "\(.capture): \({"work": "a new Work item", "link": "add to \(.work // "")", "note": "keep as a note"}[.action] // .action)\(if .workspace then " in \(.workspace | plain)" else "" end)\(if .title then ": \(.title | plain)" else "" end)", ""),
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
  local level
  level="$(jq -r '.work // 0' "$3")"
  if [ "$2" = t ] && [ "$level" -ge 10 ]; then
    echo plan:now
    return
  fi
  if [ "$2" = y ] && [ "$level" -ge 11 ]; then
    echo policies
    return
  fi
  case "$1:$2" in
    today:[1-9]*:[KJx])
      [ "$level" -ge 11 ] || return 0
      iq -r --arg row "${1#today:}" --arg key "$2" "$JQ_DEFS"'
        shown_plan | . as $p | today_rows as $rows | ($row | tonumber) as $n |
        $rows[$n - 1] // empty |
        if $key == "x" then (if not_today($p) then empty else "plan:dismiss" end)
        elif $key == "K" then (if $n > 1 then "plan:up" else empty end)
        else (if $n < ($rows | length) then "plan:down" else empty end) end' "$3"
      return
      ;;
    w-*:P)
      jq -r --arg item "$1" 'select((.work // 0) >= 10) | .workItems[]? | select(.id == $item) |
        if .pinnedAt then "work:unpin" else "work:pin" end' "$3"
      return
      ;;
    today:*) return ;;
    w-*:l) echo work:later; return ;;
    w-*:d) echo work:done; return ;;
    w-*:a) echo assist; return ;;
    c-*:w) echo sort:new; return ;;
    c-*:n) echo sort:note; return ;;
    [wc]-*:*) return ;;
  esac
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

# Today's row ids, one per line, with Today row <row> moved up or down by one:
# the order plan reorder sends (ADR 0050).
today_order() {
  iq -r --arg row "${1#today:}" --arg dir "$2" "$JQ_DEFS"'
    shown_plan | [today_rows[] | row_id] as $ids | ($row | tonumber - 1) as $i |
    (if $dir == "up" then $i - 1 else $i + 1 end) as $j |
    if $i < 0 or $j < 0 or $j >= ($ids | length) then empty
    else $ids | .[$i] as $a | .[$j] as $b | .[$i] = $b | .[$j] = $a | .[] end' "$3"
}

# Asks which staff member takes a Work item's request (ADR 0051 §2), when its
# workspace has more than one. Prints the name; Enter prints nothing: the lead.
pick_worker() {
  local names choice n=0 name what
  names="$(jq -r --arg item "$1" 'select((.work // 0) >= 12) | . as $r |
    [.workItems[]? | select(.id == $item) | .workspace][0] as $ws |
    [$r.assistant.delegates[]? | select(.workspace == $ws) | (.staff // [])[]] |
    select(length > 1) | .[] |
    "\(.worker)\t\(if .lead then "lead · " else "" end)\(.description | gsub("[\\x00-\\x1f\\x7f]"; " ") | .[:80])"' "$2")"
  [ -n "$names" ] || return 0
  clear >&2
  printf 'Which staff member?\n\n' >&2
  while IFS=$'\t' read -r name what; do
    n=$((n + 1))
    printf '  %d  %s  \033[2m%s\033[0m\n' "$n" "$name" "$what" >&2
  done <<NAMES
$names
NAMES
  printf '\n' >&2
  IFS= read -r -e -p 'Number or name (Enter: the lead decides) > ' choice || return 1
  case "$choice" in
    "") return 0 ;;
    *[!0-9]*) name="$(printf '%s\n' "$names" | cut -f1 | grep -Fx -- "$choice")" ;;
    *) name="$(printf '%s\n' "$names" | sed -n "${choice}p" | cut -f1)" ;;
  esac
  [ -n "$name" ] || { printf 'No staff member %s.\n' "$choice" >&2; return 1; }
  printf '%s\n' "$name"
}

# Asks once more before a write leaves the host (ADR 0052 §7): y approves.
confirm_write() {
  local yes
  clear
  printf 'Write to %s?\n\nThe writer posts the content exactly as the item shows it.\n\n' \
    "$(printf '%s' "$1" | tr -d '\000-\037\177')"
  IFS= read -r -n 1 -p 'y approve, n cancel > ' yes || return 1
  [ "$yes" = y ]
}

# Asks once more before a Task's text joins the workspace memory (ADR 0053 §7).
confirm_memory() {
  local yes
  clear
  printf 'Add to the memory of %s?\n\nEvery later Task and staff run of it reads the text exactly as the item shows it.\n\n' \
    "$(printf '%s' "$1" | tr -d '\000-\037\177')"
  IFS= read -r -n 1 -p 'y approve, n cancel > ' yes || return 1
  [ "$yes" = y ]
}

# Asks for an optional reason. Prints it; Enter prints nothing.
ask_reason() {
  local text
  printf '%s\n\n' "$1" >&2
  IFS= read -r -e -p 'Reason (optional, Enter to skip) > ' text || return 1
  printf '%s' "$text"
}

# What answer() asks before an action's text.
text_prompt() {
  case "$1" in
    ask-agent) echo "Request for the workspace's agent, with this item (the item stays open):" ;;
    reply) if [ "$2" = ask ]; then echo "Deny, then tell the agent instead:"; else echo "Message for the agent:"; fi ;;
  esac
}

# Runs one answer of the item in <json>. Asks for the text an action needs.
answer() {
  local id="$1" act="$2" json="$3" text err kind why row
  set -- inbox answer "$id"
  case "$act" in
    work:*) set -- work "${act#work:}" "$id" ;;
    # The item's workspace, scope research: the apps' Ask Assistant.
    assist)
      text="$(pick_worker "$id" "$json")" || { pause; return 0; }
      set -- assist "$id"
      [ -z "$text" ] || set -- "$@" --worker "$text"
      ;;
    sort:*) set -- work sort "$id" "${act#sort:}" ;;
    plan:now)
      clear
      printf 'A plan now from the overall assistant.\n\n'
      IFS= read -r -e -p 'Minutes you have (Enter: no limit) > ' text || return 0
      set -- plan now
      case "$text" in
        "") ;;
        *[!0-9]*) printf '\nNot a number of minutes.\n'; pause; return 0 ;;
        *) set -- plan now --minutes "$text" ;;
      esac
      ;;
    plan:up | plan:down)
      set -- plan reorder
      while IFS= read -r text; do
        [ -n "$text" ] && set -- "$@" "$text"
      done <<EOF
$(today_order "$id" "${act#plan:}" "$json")
EOF
      [ $# -gt 2 ] || return 0
      ;;
    plan:dismiss)
      clear
      row="$(today_target "$id" "$json")"
      [ -n "$row" ] || return 0
      text="$(ask_reason "Not today: the row stays in the plan as one line.")" || return 0
      set -- plan dismiss "$row"
      [ -z "$text" ] || set -- "$@" --reason "$text"
      ;;
    policies) "$S/policies.sh" ui; return 0 ;;
    choose:*) set -- "$@" choose --option "${act#choose:}" ;;
    approve)
      text="$(jq -r --arg item "$id" '.items[] | select(.id == $item and .kind == "write") | .write.target' "$json")"
      set -- "$@" approve
      if [ -n "$text" ]; then
        confirm_write "$text" || return 0
        # The host approves only the write whose hash this view showed (ADR 0052 §2).
        set -- "$@" --hash "$(jq -r --arg item "$id" '.items[] | select(.id == $item) | .write.hash' "$json")"
      fi
      text="$(jq -r --arg item "$id" '.items[] | select(.id == $item and .kind == "memory") | .workspace' "$json")"
      if [ -n "$text" ]; then
        confirm_memory "$text" || return 0
        # The host appends only the text whose hash this view showed (ADR 0053 §3).
        set -- "$@" --hash "$(jq -r --arg item "$id" '.items[] | select(.id == $item) | .memory.hash' "$json")"
      fi
      ;;
    reject)
      kind="$(jq -r --arg item "$id" 'select((.work // 0) >= 11) | .items[] | select(.id == $item) | .kind' "$json")"
      case "$kind" in
        sort | delegate | hypothesis | write)
          clear
          text="$(ask_reason "Reject. A reason helps the next plan; it is never required.")" || return 0
          set -- "$@" reject
          [ -z "$text" ] || set -- "$@" --reason "$text"
          ;;
        *) set -- "$@" reject ;;
      esac
      ;;
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

# What a Today row opens: the Work item or Inbox item its plan row names, or
# the first item due soon.
today_target() {
  iq -r --arg row "${1#today:}" "$JQ_DEFS"'
    shown_plan |
    if $row == "due" then (.dueSoon // [])[0] // empty
    elif $row == "" then empty
    else today_rows[$row | tonumber - 1] // empty | row_id end' "$2"
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
  local client="$1" focus="${2:-}" json rows pos picked key id act sid target
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
      --bind 'space:preview-page-down,b:preview-page-up,shift-down:preview-down,shift-up:preview-up' \
      --expect 'enter,ctrl-r,a,c,d,s,r,x,l,n,w,o,v,p,P,t,K,J,y,D,1,2,3,4,5,6,7,8,9')" || break
    key="$(printf '%s\n' "$picked" | sed -n '1p')"
    id="$(printf '%s\n' "$picked" | sed -n '2p' | cut -f1)"
    focus="$id"
    [ -n "$id" ] || continue
    case "$key" in
      ctrl-r) continue ;;
      v) read_item "$id" "$json" ;;
      D) delete_old "$json" ;;
      enter | o)
        case "$id" in
          today:*)
            # Moves to the row of the target; a target not listed stays put.
            target="$(today_target "$id" "$json")"
            if [ -n "$target" ] && printf '%s\n' "$rows" | cut -f1 | grep -qxF -- "$target"; then
              focus="$target"
            fi
            continue
            ;;
        esac
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
          *)
            answer "$id" "$act" "$json"
            # The moved row keeps the cursor.
            case "$act" in
              plan:up) focus="today:$((${id#today:} - 1))" ;;
              plan:down) focus="today:$((${id#today:} + 1))" ;;
            esac
            ;;
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
  target) today_target "${2:?today row}" "${3:?json}" ;;
  key) key_action "${2:?item id}" "${3:?key}" "${4:?json}" ;;
  order) today_order "${2:?today row}" "${3:?up or down}" "${4:?json}" ;;
  worker) pick_worker "${2:?work id}" "${3:?json}" ;;
  ui) tty_utf8; ui "${2:?client}" "${3:-}" ;;
  *) echo "usage: inbox.sh cache [--force] | rows | detail <target> | list <json> | view <id> <json> | target <today row> <json> | key <id> <key> <json> | order <today row> up|down <json> | worker <work id> <json> | ui <client> [item id]" >&2; exit 2 ;;
esac
