#!/usr/bin/env bash
# shellcheck disable=SC2016  # jq programs: $ is jq's, not the shell's
# Assistant results the user has not looked at: prefix + T marks their Work
# items, and the Inbox poll notifies once per result. muxa never sees this
# state; it lives in one file on this machine (results_file).
# Usage: results.sh track <inbox json>  (the poll: record and notify)
#        results.sh seen <work id> <inbox json> | new <inbox json>
set -u
. "$(dirname "$0")/lib.sh"

STATE="$(results_file)"
LOCK="$STATE.lock"
# A lock older than this, in seconds, belongs to a writer that died.
LOCK_LIMIT=10

# A result is its request's id, version and delivery time: a new version or a
# new delivery of the same version is a new result.
JQ_RESULTS='
  def results: (.requests // []) as $reqs | (.workItems // [])[] |
    select(.state != "done" and .state != "withdrawn") | . as $w |
    (.requests // [])[-1] as $q | [$reqs[] | select(.id == $q)][0] // empty |
    select(.delivery == "resulted") |
    {work: $w.id, key: "\(.id) v\(.version) \(.deliveryAt // .updatedAt // 0)",
     title: ($w.title // $w.id | gsub("[\\x00-\\x1f\\x7f]"; ""))};
'

mtime() { stat -f %m "$1" 2>/dev/null || stat -c %Y "$1" 2>/dev/null || echo 0; }

lock() {
  local _
  mkdir -p "$(dirname "$STATE")" || return 1
  for _ in 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 18 19 20; do
    mkdir "$LOCK" 2>/dev/null && return 0
    if [ $(($(date +%s) - $(mtime "$LOCK"))) -ge "$LOCK_LIMIT" ]; then
      rmdir "$LOCK" 2>/dev/null
      continue
    fi
    sleep 0.1
  done
  log "results: $LOCK is held; skipped"
  return 1
}
unlock() { rmdir "$LOCK" 2>/dev/null; }

# The saved state, {"<work id>": {"key", "seen"}}, or {} when there is none.
saved() { if [ -s "$STATE" ]; then cat "$STATE"; else echo '{}'; fi; }

# Replaces the state with stdin, atomically; an unchanged state is not written.
save() {
  local tmp
  tmp="$(mktemp "$STATE.XXXXXX")" || return 1
  cat >"$tmp"
  if ! jq -e 'type == "object"' "$tmp" >/dev/null 2>&1; then
    rm -f "$tmp"
    log "results: refused to save a state that is not an object"
    return 1
  fi
  if cmp -s "$tmp" "$STATE"; then rm -f "$tmp"; else mv "$tmp" "$STATE"; fi
}

# The poll: keeps one entry per open Work item with a result, so Work items
# no longer listed drop out. A result not saved yet is new and notifies once.
# The first run saves what is there as seen: no notifications for old results.
track() {
  local json="$1" first=false out title
  [ -s "$json" ] || return 0
  lock || return 0
  [ -f "$STATE" ] || first=true
  out="$(saved | jq -r --argjson first "$first" --slurpfile inbox "$json" "$JQ_RESULTS"'
    . as $old | [$inbox[0] | results] as $now |
    [$now[] | select($old[.work].key != .key)] as $fresh |
    (reduce $now[] as $r ({}; .[$r.work] =
      (if $old[$r.work].key == $r.key then $old[$r.work] else {key: $r.key, seen: $first} end))
     | tojson),
    (if $first then empty else $fresh[] | .title end)')" || { unlock; return 1; }
  printf '%s\n' "$out" | head -n 1 | save
  unlock
  printf '%s\n' "$out" | tail -n +2 | while IFS= read -r title; do
    [ -n "$title" ] && "$AGENTS_SCRIPTS/notify.sh" --result "$title" </dev/null >/dev/null 2>&1
  done
  return 0
}

# The user looked at a Work item: its current result is seen. Before the first
# poll there is nothing to mark.
seen() {
  local work="$1" json="$2" next
  [ -f "$STATE" ] || return 0
  lock || return 0
  next="$(saved | jq -c --arg work "$work" --slurpfile inbox "$json" "$JQ_RESULTS"'
    [$inbox[0] | results | select(.work == $work)][0] as $r |
    if $r then .[$work] = {key: $r.key, seen: true} else . end')" &&
    printf '%s\n' "$next" | save
  unlock
}

# The Work ids with a result not looked at, as a JSON array.
new() {
  saved | jq -c --slurpfile inbox "$1" "$JQ_RESULTS"'
    . as $s | [$inbox[0] | results | select($s[.work].key == .key and ($s[.work].seen | not)) | .work]
  ' 2>/dev/null || echo '[]'
}

case "${1:-}" in
  track) track "${2:?inbox json}" ;;
  seen) seen "${2:?work id}" "${3:?inbox json}" ;;
  new) new "${2:?inbox json}" ;;
  *) echo "usage: results.sh track <json> | seen <work id> <json> | new <json>" >&2; exit 2 ;;
esac
