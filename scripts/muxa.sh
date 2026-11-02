#!/usr/bin/env bash
# Tasks through muxa, when it is installed on this host: the Inbox popup, and
# new tasks that are muxa task records, the same ones its iPad and phone apps
# show. muxa owns the records and the Inbox rule; this only renders them.
# Usage: muxa.sh available | tasks | rows | create <workspace> <repo> <branch> <title> <goal>
#        muxa.sh inbox <client> | new <client> <repo>    (open a popup)
#        muxa.sh inbox-ui <client> | form <client> <repo> (run inside it)
set -u
. "$(dirname "$0")/lib.sh"

S="$AGENTS_SCRIPTS"

# muxa-cli from @agents_muxa_cli, else PATH. Nothing when @agents_muxa is off.
muxa_cli() {
  local cli
  [ "$(opt @agents_muxa auto)" = off ] && return 1
  cli="$(opt @agents_muxa_cli '')"
  [ -n "$cli" ] || cli="$(command -v muxa-cli 2>/dev/null)" || return 1
  [ -x "$cli" ] && printf '%s' "$cli"
}

# The bridge muxa-cli starts: @agents_muxa_bridge, else the one beside
# muxa-cli, on this plugin's tmux server.
bridge_cmd() {
  local cli="$1" bridge
  bridge="$(opt @agents_muxa_bridge '')"
  if [ -z "$bridge" ]; then
    bridge="$(dirname "$cli")/muxa-bridge"
    [ -x "$bridge" ] || bridge=muxa-bridge
    bridge="$(printf '%q' "$bridge")"
  fi
  [ -n "${TMUX_AGENTS_SOCKET:-}" ] && bridge="$bridge --socket $(printf '%q' "$TMUX_AGENTS_SOCKET")"
  printf '%s' "$bridge"
}

run_muxa() {
  local cli
  cli="$(muxa_cli)" || { echo "muxa-cli is not installed" >&2; return 1; }
  "$cli" --bridge "$(bridge_cmd "$cli")" "$@"
}

# The Inbox as muxa-cli prints it: {"workspaces", "truncated", "tasks"}.
tasks() { run_muxa tasks --json </dev/null; }

# One row per Task, in muxa's order: <session id> TAB <display>. The group's
# name heads its first row, so every row is a Task. No session: no id.
rows() {
  local w e r i d
  IFS=$'\037' read -r w e r i d <<EOF
$(t display-message -p "#{@agents_icon_waiting}"$'\037'"#{@agents_icon_error}"$'\037'"#{@agents_icon_running}"$'\037'"#{@agents_icon_idle}"$'\037'"#{@agents_icon_done}" 2>/dev/null)
EOF
  jq -r '.tasks[] | [.group, (.session.id // ""), (.agent.state // ""), .record.title,
                     (.session.name // .record.session // ""), .record.id, (.need // "")] | @tsv' |
    awk -F'\t' -v IW="${w:-🙋}" -v IE="${e:-❌}" -v IR="${r:-⚡}" -v II="${i:-💤}" -v ID="${d:-🏁}" '
      BEGIN {
        title["needs-attention"] = "Needs attention"; title["ready-for-review"] = "Ready for review"
        title["running"] = "Running"; title["no-agent"] = "No agent"; title["closed"] = "Closed"
        need["answer"] = "waits for your answer: Enter to open"
        need["agent-error"] = "agent error"; need["loop"] = "loop needs attention"
        need["review"] = "ready for review"; need["no-agent"] = "no agent state is available"
        need["no-session"] = "no session"
        icon["waiting"] = IW; icon["error"] = IE; icon["running"] = IR; icon["idle"] = II; icon["done"] = ID
        D = "\033[2m"; BO = "\033[1m"; Z = "\033[0m"
      }
      {
        group = ($1 == last ? "" : ($1 in title ? title[$1] : $1)); last = $1
        ic = ($3 in icon ? icon[$3] : "  ")
        where = ($2 == "" ? D "no session" Z : $5)
        printf "%s\t%s%-16s%s  %s %s  %s%s%s  %s%s\n", $2, BO, group, Z, ic, $4, D, $6, Z, where, ($7 in need ? "  " need[$7] : "")
      }'
}

# Starts a Task through muxa and prints its session id. muxa-cli prints the
# Task first, then waits until the agent got its start prompt; that wait goes
# on in the background, immune to the popup closing.
create() {
  local ws="$1" repo="$2" branch="$3" title="$4" goal="${5:-}" cli line err
  cli="$(muxa_cli)" || { fail "muxa-cli is not installed"; return 1; }
  [ -n "$goal" ] || goal="$title"
  set -- task new --workspace "$ws" --title "$title" --goal "$goal" --json
  [ -n "$repo" ] && set -- "$@" --repo "$repo" --branch "$branch"
  err="$(agents_run_dir)/muxa-$$.err"
  exec 3< <(nohup "$cli" --bridge "$(bridge_cmd "$cli")" "$@" </dev/null 2>"$err" 3>&-)
  IFS= read -r line <&3 || line=""
  exec 3<&-
  if [ -z "$line" ]; then
    wait_for_file "$err"
    fail "muxa could not start the task: $(sed 's/^muxa-cli: //' "$err" | tail -n 1)"
    rm -f "$err"
    return 1
  fi
  rm -f "$err"
  notify_change
  printf '%s' "$line" | jq -r '.session.id // empty'
}

# To the caller and the log; the form shows it in its popup.
fail() { log "$*"; echo "$*" >&2; }

# muxa-cli writes its error when it exits, after closing stdout.
wait_for_file() {
  local n=0
  while [ ! -s "$1" ] && [ "$n" -lt 20 ]; do sleep 0.1; n=$((n + 1)); done
}

# Moves <client> to the session with this id.
go() {
  t switch-client -c "$1" -t "$2" || return 1
  notify_change
}

popup() {
  local client="$1" title="$2"
  shift 2
  # The popup gets the server's environment, not this script's.
  [ -n "${TMUX_AGENTS_SOCKET:-}" ] && set -- env "TMUX_AGENTS_SOCKET=$TMUX_AGENTS_SOCKET" "$@"
  t display-popup -c "$client" -E -w 80% -h 70% -T " $title " "$(printf '%q ' "$@")"
}

inbox() {
  muxa_cli >/dev/null || die "muxa is not installed; the Inbox needs muxa-cli on this host"
  popup "$1" "Inbox" "$S/muxa.sh" inbox-ui "$1"
}

inbox_ui() {
  local client="$1" json picked sid
  while :; do
    json="$(tasks)" || { echo "muxa could not list the tasks. Press Enter."; read -r _; return 1; }
    picked="$(printf '%s' "$json" | rows | fzf --ansi --delimiter '\t' --with-nth 2.. --no-sort \
      --layout reverse --info hidden --prompt 'task> ' \
      --header 'Enter go to the task  Esc close  ctrl-r reload' \
      --expect ctrl-r)" || return 0
    case "$picked" in ctrl-r*) continue ;; esac
    sid="$(printf '%s\n' "$picked" | sed -n '2p' | cut -f1)"
    if [ -n "$sid" ]; then go "$client" "$sid"; return; fi
    echo "That Task has no session. Open its detail in muxa to mark it Done or Dropped."
    sleep 2
  done
}

new() {
  muxa_cli >/dev/null || die "muxa is not installed"
  popup "$1" "New task" "$S/muxa.sh" form "$1" "$2"
}

# Without a muxa workspace: a plain task, as without muxa.
plain_form() {
  local client="$1" repo="$2" branch name
  echo "muxa has no workspace (~/.config/muxa/workspaces.toml), so this task gets no muxa record."
  repo="$(ask repo "$repo")" || return 0
  branch="$(ask branch "")" || return 0
  [ -n "$branch" ] || return 0
  if name="$("$S/new.sh" "$repo" "$branch" --no-switch 2>&1)"; then
    go "$client" "=$name"
  else
    printf '%s\nSee %s. Press Enter.\n' "$name" "$(agents_log_file)"
    read -r _
  fi
}

# "label [default]: " -> the answer, or the default when empty.
ask() {
  local answer
  printf '%s%s: ' "$1" "${2:+ [$2]}" >&2
  IFS= read -r answer || return 1
  printf '%s' "${answer:-$2}"
}

form() {
  local client="$1" repo="$2" json spaces ws branch title goal sid
  json="$(tasks 2>/dev/null)" || json=""
  spaces="$(printf '%s' "$json" | jq -r '.workspaces // [] | join(" ")' 2>/dev/null)"
  if [ -z "$spaces" ]; then
    plain_form "$client" "$repo"
    return
  fi
  ws="$(opt @agents_muxa_workspace '')"
  case " $spaces " in *" $ws "*) ;; *) ws="${spaces%% *}" ;; esac
  echo "A muxa task: the same record on every device. Workspaces: $spaces"
  echo "Repo '-' starts a task without a repo, in its workspace."
  ws="$(ask workspace "$ws")" || return 0
  repo="$(ask repo "$repo")" || return 0
  [ "$repo" = - ] && repo=""
  if [ -n "$repo" ]; then
    branch="$(ask branch "")" || return 0
    [ -n "$branch" ] || { echo "A repo task needs a branch."; sleep 2; return 1; }
  fi
  title="$(ask title "${branch:-}")" || return 0
  goal="$(ask goal "$title")" || return 0
  echo "Starting..."
  if sid="$(create "$ws" "$repo" "${branch:-}" "$title" "$goal" 2>&1)" && [ -n "$sid" ]; then
    go "$client" "$sid"
  else
    printf '%s\nPress Enter.\n' "${sid:-muxa started the task, but it has no session}"
    read -r _
  fi
}

case "${1:-}" in
  available) muxa_cli >/dev/null ;;
  tasks) tasks ;;
  rows) rows ;;
  create) shift; create "${1:-}" "${2:-}" "${3:-}" "${4:-}" "${5:-}" ;;
  inbox) inbox "${2:?client}" ;;
  inbox-ui) inbox_ui "${2:?client}" ;;
  new) new "${2:?client}" "${3:-}" ;;
  form) form "${2:?client}" "${3:-}" ;;
  *) echo "usage: muxa.sh available | tasks | rows | create <workspace> <repo> <branch> <title> <goal> | inbox <client> | new <client> <repo>" >&2; exit 2 ;;
esac
