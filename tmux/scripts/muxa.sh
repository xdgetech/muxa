#!/usr/bin/env bash
# Tasks through muxa, when it is installed on this host: the Inbox popup, and
# new tasks that are muxa task records, the same ones its iPad and phone apps
# show. muxa owns the records and the Inbox rule; this only renders them.
# Usage: muxa.sh available | tasks | rows | create <workspace> <repo> <branch> <title> <goal> [worker]
#        muxa.sh close-task <task id> <done|dropped>
#        muxa.sh repo-rows <workspace> <default repo> [none] | worker-rows <workspace> | start-ask <workspace> <repo>
#        muxa.sh inbox <client> [item] | new <client> <repo> | ask <client> <repo>  (open a popup)
#        muxa.sh cli <muxa-cli args> | goto <client> <session id>
#        muxa.sh inbox-ui <client> | form <client> <repo> | ask-ui <client> <json file> <repo>
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

# One row per Task, in muxa's order: <session id> TAB <task id> TAB <title>
# TAB <display>. The group's name heads its first row, so every row is a Task.
# No session: no session id.
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
        printf "%s\t%s\t%s\t%s%-16s%s  %s %s  %s%s%s  %s%s\n", $2, $6, $4, BO, group, Z, ic, $4, D, $6, Z, where, ($7 in need ? "  " need[$7] : "")
      }'
}

# Starts a Task through muxa and prints its session id. muxa-cli prints the
# Task first, then waits until the agent got its start prompt; that wait goes
# on in the background, immune to the popup closing.
create() {
  local ws="$1" repo="$2" branch="$3" title="$4" goal="${5:-}" worker="${6:-}" cli line err
  cli="$(muxa_cli)" || { fail "muxa-cli is not installed"; return 1; }
  [ -n "$goal" ] || goal="$title"
  set -- task new --workspace "$ws" --title "$title" --goal "$goal" --json
  [ -n "$repo" ] && set -- "$@" --repo "$repo"
  # Without a branch, a work:2 host names one after the title.
  [ -n "$repo" ] && [ -n "$branch" ] && set -- "$@" --branch "$branch"
  [ -n "$worker" ] && set -- "$@" --worker "$worker"
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

# The Inbox of ADR 0033 (inbox.sh), which falls back to the Task list on an older muxa.
inbox() {
  muxa_cli >/dev/null || die "muxa is not installed; the Inbox needs muxa-cli on this host"
  popup "$1" "Inbox" "$S/inbox.sh" ui "$1" "${2:-}"
}

inbox_ui() {
  local client="$1" json picked key row sid task title
  while :; do
    json="$(tasks)" || { echo "muxa could not list the tasks. Press Enter."; read -r _; return 1; }
    picked="$(printf '%s' "$json" | rows | fzf --ansi --delimiter '\t' --with-nth 4.. --no-sort \
      --layout reverse --info hidden --prompt 'task> ' \
      --header 'Enter go to the task  ctrl-d Done  ctrl-x Drop  ctrl-r reload  Esc close' \
      --expect ctrl-r,ctrl-d,ctrl-x)" || return 0
    key="$(printf '%s\n' "$picked" | sed -n '1p')"
    row="$(printf '%s\n' "$picked" | sed -n '2p')"
    sid="$(printf '%s' "$row" | cut -f1)"
    task="$(printf '%s' "$row" | cut -f2)"
    title="$(printf '%s' "$row" | cut -f3)"
    [ -n "$task" ] || continue
    case "$key" in
      ctrl-r) continue ;;
      ctrl-d) confirm_close "$task" "$title" "done" ;;
      ctrl-x) confirm_close "$task" "$title" dropped ;;
      *)
        if [ -n "$sid" ]; then go "$client" "$sid"; return; fi
        no_session "$task" "$title" ;;
    esac
  done
}

# Enter on a Task whose session is gone: close it here, or go back.
no_session() {
  local answer
  printf '"%s" has no session.\nd: mark it Done   x: Drop it   any other key: back ' "$2"
  IFS= read -r -n 1 answer || return 0
  echo
  case "$answer" in
    d) close_task "$1" "done" ask || pause ;;
    x) close_task "$1" dropped ask || pause ;;
  esac
}

confirm_close() {
  local answer verb="mark \"$2\" Done"
  [ "$3" = dropped ] && verb="drop \"$2\""
  printf '%s? Its session closes, and its worktree and branch go. (y/n) ' "$verb"
  IFS= read -r -n 1 answer || return 0
  echo
  [ "$answer" = y ] || return 0
  close_task "$1" "$3" ask || pause
}

pause() { echo "Press Enter."; read -r _; }

# Marks a Task record done or dropped. muxa keeps the record, closes the
# session and removes the worktree and branch (ADR 0043). When that could lose
# work, muxa changes nothing; with <ask> the user may say to remove them anyway.
close_task() {
  local task="$1" status="$2" ask="${3:-}" verb err why
  case "$status" in
    done) verb="done" ;;
    dropped) verb=drop ;;
    *) fail "close-task takes done or dropped, not $status"; return 2 ;;
  esac
  case "$task" in
    t-*) ;;
    *) fail "not a muxa task id: $task"; return 2 ;;
  esac
  if ! err="$(run_muxa task "$verb" "$task" --json 2>&1 >/dev/null)"; then
    why="$(lost_work "$err")"
    if [ -n "$why" ] && [ -n "$ask" ]; then
      confirm_loss "$why" || return 0
      err="$(run_muxa task "$verb" "$task" --force --json 2>&1 >/dev/null)" && { notify_change; return 0; }
    fi
    fail "muxa could not mark $task $status: $(printf '%s' "$err" | sed 's/^muxa-cli: //' | tail -n 1)"
    return 1
  fi
  notify_change
}

new() {
  muxa_cli >/dev/null || die "muxa is not installed"
  popup "$1" "New task" "$S/muxa.sh" form "$1" "$2"
}

# Without a muxa workspace: a plain task, as without muxa.
plain_form() {
  local client="$1" repo="$2" branch name
  echo "muxa has no workspace (~/.config/muxa/workspaces.toml), so this task gets no muxa record."
  repo="$(read_answer repo "$repo")" || return 0
  branch="$(read_answer branch "")" || return 0
  [ -n "$branch" ] || return 0
  if name="$("$S/new.sh" "$repo" "$branch" --no-switch 2>&1)"; then
    go "$client" "=$name"
  else
    printf '%s\nSee %s. Press Enter.\n' "$name" "$(agents_log_file)"
    read -r _
  fi
}

# "label [default]: " -> the answer, or the default when empty.
read_answer() {
  local answer
  printf '%s%s: ' "$1" "${2:+ [$2]}" >&2
  IFS= read -r answer || return 1
  printf '%s' "${answer:-$2}"
}

# Work level 8 (ADR 0046): the text comes first. s saves it as a Capture and
# starts nothing; a saves it and asks a workspace's assistant (scope research);
# t takes it into the New Task form. Sets SEED for the form.
# Returns 1 when nothing more is to be done.
capture_first() {
  local json="$1" spaces="$2" repo="$3" text choice ws out=""
  SEED=""
  echo "What is on your mind? It is saved on the host; nothing starts unless you ask."
  echo "Leave it empty for the New Task form."
  text="$(read_answer text "")" || return 1
  [ -n "$text" ] || return 0
  printf 's save   a save and ask the assistant   t start a Task with it   q cancel: '
  IFS= read -r -n 1 choice || return 1
  echo
  case "$choice" in
    s | "")
      out="$(save_capture "$text")" &&
        echo "Saved in Unsorted: $(printf '%s' "$out" | jq -r '.capture.id' 2>/dev/null)"
      ;;
    a)
      ws="$(choose_ws "$spaces" "$(default_ws "$json" "$repo")")" || return 1
      [ -n "$ws" ] || return 1
      out="$(save_capture "$text" --assist --workspace "$ws")" &&
        printf 'Asked the assistant in %s: %s\n' "$ws" \
          "$(printf '%s' "$out" | jq -r '"\(.request.id // "no request") (\(.request.delivery // "unknown"))"' 2>/dev/null)"
      ;;
    t) SEED="$text" ;;
  esac
  [ -n "$SEED" ] || { [ -z "$out" ] || sleep 1; return 1; }
}

# Sends <text> as a Capture with the flags after it and prints muxa's JSON.
# On an error, shows it and waits for Enter.
save_capture() {
  local text="$1" out
  shift
  # On stdin: the text is never read as a flag.
  if out="$(printf '%s' "$text" | run_muxa capture - --source tmux "$@" --json 2>&1)"; then
    printf '%s' "$out"
    "$S/inbox.sh" cache --force </dev/null >/dev/null 2>&1 &
  else
    printf '%s\nPress Enter.\n' "$(printf '%s' "$out" | sed 's/^muxa-cli: //' | tail -n 1)" >&2
    read -r _
    return 1
  fi
}

form() {
  local client="$1" repo="$2" json spaces ws branch="" title goal sid w2="" workers worker=""
  json="$(tasks 2>/dev/null)" || json=""
  spaces="$(printf '%s' "$json" | ws_names)"
  if [ -z "$spaces" ]; then
    plain_form "$client" "$repo"
    return
  fi
  SEED=""
  if printf '%s' "$json" | work_at 8; then
    capture_first "$json" "$spaces" "$repo" || return 0
  fi
  printf '%s' "$json" | work2 && w2=1
  echo "A muxa task: the same record on every device. Workspaces: $spaces"
  ws="$(choose_ws "$spaces" "$(default_ws "$json" "$repo")")" || return 0
  repo="$(pick_repo "$(repo_rows "$json" "$ws" "$repo" none)" "Enter pick  - no repo, in the workspace  Esc cancel")" || return 0
  [ "$repo" = - ] && repo=""
  [ -n "$repo" ] && echo "repo: $repo"
  if [ -n "$repo" ] && [ -n "$w2" ]; then
    branch="$(read_answer "branch (empty: named after the title)" "")" || return 0
  elif [ -n "$repo" ]; then
    branch="$(read_answer branch "")" || return 0
    [ -n "$branch" ] || { echo "A repo task needs a branch: this muxa cannot name one."; sleep 2; return 1; }
  fi
  title="$branch"
  [ -n "$SEED" ] && title="$(printf '%s\n' "$SEED" | head -n 1 | cut -c 1-72)"
  title="$(read_answer title "$title")" || return 0
  [ -n "$title" ] || { echo "A task needs a title."; sleep 2; return 1; }
  goal="$(read_answer goal "${SEED:-$title}")" || return 0
  workers="$(worker_rows "$json" "$ws")"
  if [ -n "$workers" ]; then
    worker="$(pick_worker "$workers")" || return 0
    echo "agent: $worker"
  fi
  echo "Starting..."
  if sid="$(create "$ws" "$repo" "$branch" "$title" "$goal" "$worker" 2>&1)" && [ -n "$sid" ]; then
    go "$client" "$sid"
  else
    printf '%s\nPress Enter.\n' "${sid:-muxa started the task, but it has no session}"
    read -r _
  fi
}

# Workspace names: plain strings from older muxa-cli, else {name, root?, repos?}.
# muxa-cli prints the objects in workspaceInfo and keeps workspaces as names;
# The readers below take the objects from either.
ws_names() {
  jq -r '.workspaces // [] | map(if type == "string" then . else .name end) | join(" ")' 2>/dev/null
}

# Whether the host has muxa's work level 2 (ADR 0031): repos relative to the
# root, a branch named after the title, and ask sessions. A listed capability
# decides; else a workspace root, which came in the same muxa release.
work2() {
  jq -e '((.capabilities // []) | map(select(type == "string" and test("^work:[0-9]+$")) | ltrimstr("work:") | tonumber) | max // 0) >= 2
         or any((.workspaceInfo // .workspaces // [])[]?; type == "object" and .root != null)' >/dev/null 2>&1
}

# Whether the host has muxa's work level <n> or higher, by its capabilities.
work_at() {
  jq -e --argjson n "$1" '((.capabilities // []) | map(select(type == "string" and test("^work:[0-9]+$")) | ltrimstr("work:") | tonumber) | max // 0) >= $n' >/dev/null 2>&1
}

# <dir> with symlinks resolved, or nothing when it does not exist.
real_dir() { [ -n "$1" ] && (cd "$1" 2>/dev/null && pwd -P); }

# The first "<path> TAB ..." line on stdin whose path is <dir>, symlinks
# resolved: muxa and git may each name a repo through another link.
line_for_dir() {
  local want path rest
  want="$(real_dir "$1")" || return 0
  while IFS="$TAB" read -r path rest; do
    [ "$(real_dir "$path")" = "$want" ] && { printf '%s\t%s\n' "$path" "$rest"; return; }
  done
}

# The workspace whose repos hold <repo>, else @agents_muxa_workspace, else the first.
default_ws() {
  local json="$1" spaces owner ws
  spaces="$(printf '%s' "$json" | ws_names)"
  owner="$(printf '%s' "$json" | jq -r '(.workspaceInfo // .workspaces // [])[]? | objects | .name as $n | (.repos // [])[] | [., $n] | @tsv' 2>/dev/null |
    line_for_dir "$2" | cut -f2)"
  [ -n "$owner" ] && { printf '%s' "$owner"; return; }
  ws="$(opt @agents_muxa_workspace '')"
  case " $spaces " in *" $ws "*) ;; *) ws="${spaces%% *}" ;; esac
  printf '%s' "$ws"
}

# The workspace: the only one, else asked with <default>.
choose_ws() {
  case "$1" in *" "*) read_answer workspace "$2" ;; *) printf '%s' "$1" ;; esac
}

# Picker rows "<path> TAB <shown>" for workspace <ws>: <default repo> first,
# then the workspace's repos. A repo under the root shows relative to it,
# others with ~ for $HOME. "none" adds "-", no repo.
repo_rows() {
  local json="$1" ws="$2" def="$3" rows first
  rows="$(printf '%s' "$json" | jq -r --arg ws "$ws" --arg home "$HOME" '
    (.workspaceInfo // .workspaces // [])[]? | objects | select(.name == $ws) | (.root // "" | rtrimstr("/")) as $root |
    (.repos // [])[] | select(type == "string") |
    if $root != "" and startswith($root + "/") then [., ltrimstr($root + "/")]
    elif startswith($home + "/") then [., "~/" + ltrimstr($home + "/")]
    else [., .] end | @tsv' 2>/dev/null)"
  first="$(printf '%s\n' "$rows" | line_for_dir "$def")"
  if [ -z "$first" ] && [ -n "$def" ] && git -C "$def" rev-parse --git-dir >/dev/null 2>&1; then
    case "$def" in "$HOME"/*) first="$def$TAB~/${def#"$HOME"/}" ;; *) first="$def$TAB$def" ;; esac
  fi
  { [ -n "$first" ] && printf '%s\n' "$first"
    [ -n "$rows" ] && printf '%s\n' "$rows" | grep -vxF -- "$first"
    [ "${4:-}" = none ] && printf -- '-\t- no repo\n'
  } | awk 'NF'
}

# Picker rows "<worker> TAB <shown>" for workspace <ws>: "default", the
# workspace's own agent, first, then the host's workers that <ws> may use.
# Nothing when the host lists no such worker: the workspace's agent runs.
worker_rows() {
  local rows
  rows="$(printf '%s' "$1" | jq -r --arg ws "$2" '
    (.workers // [])[]? | objects | select(.workspaces == null or (.workspaces | index($ws)))
    | [.name, .name + "  " + .agent + (if .description then "  " + .description else "" end)] | @tsv' 2>/dev/null)"
  [ -n "$rows" ] || return 0
  printf 'default\tdefault  the workspace'"'"'s agent\n%s\n' "$rows"
}

# The picked worker's name. Fails on Esc.
pick_worker() {
  printf '%s\n' "$1" | fzf --delimiter '\t' --with-nth 2 --exact --layout reverse --info hidden \
    --prompt 'agent> ' --header 'Enter pick  Esc cancel' | cut -f1 | grep .
}

# The picked repo's path, or the typed text when no row matches it. Fails on Esc.
pick_repo() {
  local out rc q
  out="$(printf '%s\n' "$1" | awk 'NF' | fzf --delimiter '\t' --with-nth 2 --exact --print-query \
    --layout reverse --info hidden --prompt 'repo> ' --header "$2  (or type a path)")"
  rc=$?
  case "$rc" in
    0) printf '%s\n' "$out" | sed -n '2p' | cut -f1 ;;
    1) q="$(printf '%s\n' "$out" | head -n 1)"
       [ -n "$q" ] || return 1
       case "$q" in \~) q="$HOME" ;; \~/*) q="$HOME/${q#\~/}" ;; esac
       printf '%s' "$q" ;;
    *) return 1 ;;
  esac
}

# An ask session (muxa's work:2): the workspace's agent in <repo>'s main
# checkout, with no Task and no worktree. Prints its session id.
start_ask() {
  local out
  out="$(run_muxa ask "$2" --workspace "$1" --json </dev/null)" || return 1
  notify_change
  printf '%s' "$out" | jq -r '.session // empty'
}

# The checks run before the popup, so a host that cannot ask opens nothing.
ask_key() {
  local client="$1" repo="$2" json f
  muxa_cli >/dev/null || die "ask sessions need muxa: muxa-cli is not installed, or @agents_muxa is off"
  json="$(tasks 2>/dev/null)" || die "muxa could not list its workspaces; try muxa-cli tasks"
  [ -n "$(printf '%s' "$json" | ws_names)" ] || die "ask sessions need a muxa workspace (~/.config/muxa/workspaces.toml)"
  printf '%s' "$json" | work2 || die "this muxa cannot start ask sessions; update muxa-cli and muxa-bridge"
  f="$(agents_run_dir)/ask-$$.json"
  printf '%s' "$json" >"$f" || die "could not write $f"
  popup "$client" "Ask" "$S/muxa.sh" ask-ui "$client" "$f" "$repo"
}

ask_ui() {
  local client="$1" json spaces ws repo="$3" sid
  json="$(cat "$2")" || return 1
  rm -f "$2"
  spaces="$(printf '%s' "$json" | ws_names)"
  echo "Ask about a repo: an agent in its main checkout, with no task and no worktree."
  echo "The agent can change files there. Workspaces: $spaces"
  ws="$(choose_ws "$spaces" "$(default_ws "$json" "$repo")")" || return 0
  repo="$(pick_repo "$(repo_rows "$json" "$ws" "$repo")" "Enter ask in this repo  Esc cancel")" || return 0
  [ -n "$repo" ] && [ "$repo" != - ] || return 0
  echo "Starting an agent in $repo..."
  if sid="$(start_ask "$ws" "$repo" 2>&1)" && [ -n "$sid" ]; then
    go "$client" "$sid"
  else
    printf '%s\nPress Enter.\n' "$(printf '%s' "${sid:-muxa started no session}" | sed 's/^muxa-cli: //')"
    read -r _
  fi
}

case "${1:-}" in
  available) muxa_cli >/dev/null ;;
  tasks) tasks ;;
  rows) rows ;;
  create) shift; create "${1:-}" "${2:-}" "${3:-}" "${4:-}" "${5:-}" "${6:-}" ;;
  close-task) close_task "${2:?task id}" "${3:?done or dropped}" ;;
  inbox) inbox "${2:?client}" "${3:-}" ;;
  cli) shift; run_muxa "$@" ;;
  goto) go "${2:?client}" "${3:?session id}" ;;
  inbox-ui) inbox_ui "${2:?client}" ;;
  new) new "${2:?client}" "${3:-}" ;;
  form) form "${2:?client}" "${3:-}" ;;
  repo-rows) repo_rows "$(cat)" "${2:?workspace}" "${3:-}" "${4:-}" ;;
  worker-rows) worker_rows "$(cat)" "${2:?workspace}" ;;
  start-ask) start_ask "${2:?workspace}" "${3:?repo}" ;;
  ask) ask_key "${2:?client}" "${3:-}" ;;
  ask-ui) ask_ui "${2:?client}" "${3:?json file}" "${4:-}" ;;
  *) echo "usage: muxa.sh available | tasks | rows | create <workspace> <repo> <branch> <title> <goal> | close-task <task id> <done|dropped> | repo-rows <workspace> <repo> [none] | start-ask <workspace> <repo> | inbox <client> [item] | cli <args> | goto <client> <session id> | new <client> <repo> | ask <client> <repo>" >&2; exit 2 ;;
esac
