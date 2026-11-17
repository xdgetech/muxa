#!/usr/bin/env bash
# shellcheck disable=SC2016  # the shims' $ belongs to the shim, not to this shell
# Measures the tmux Inbox on a throwaway tmux server: one Inbox fetch, the
# popup's per-cursor and per-key scripts, and the idle cost of N sidebars.
# It reads a copy of a muxa state dir and never touches the default server
# or the original state.
# Usage: scripts/bench/inbox-bench.sh --state <muxa tasks dir> [--json <inbox json>]
#        [--sessions N] [--idle SECONDS] [--workspaces FILE]
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
export MUXA_SOCKET="muxa-bench-$$"
. "$ROOT/scripts/lib/tmux.bash"

state="" json="" sessions=40 idle=60 workspaces=""
while [ $# -gt 0 ]; do
  case "$1" in
    --state) state="$2"; shift 2 ;;
    --json) json="$2"; shift 2 ;;
    --sessions) sessions="$2"; shift 2 ;;
    --idle) idle="$2"; shift 2 ;;
    --workspaces) workspaces="$2"; shift 2 ;;
    *) echo "usage: $0 --state DIR [--json FILE] [--sessions N] [--idle S] [--workspaces FILE]" >&2; exit 2 ;;
  esac
done
[ -d "$state" ] || { echo "--state: a muxa tasks dir is required" >&2; exit 2; }

BIN="${MUXA_BIN:-$ROOT/target/release}"
[ -x "$BIN/muxa-cli" ] && [ -x "$BIN/muxa-bridge" ] ||
  { echo "build first: cargo build --release -p muxa-bridge -p muxa-cli" >&2; exit 2; }

work="$(mktemp -d "${TMPDIR:-/tmp}/muxa-bench.XXXXXX")"
trap 'mt_down; rm -rf "$work"' EXIT
cp -R "$state" "$work/tasks"
mkdir -p "$work/daemon" "$work/shims" "$work/xdg" "$work/tmp"

bridge="$BIN/muxa-bridge --socket $MUXA_SOCKET --state-dir $work/tasks --daemon-dir $work/daemon"
[ -z "$workspaces" ] || bridge="$bridge --workspaces $workspaces"

# Shims that log one line per call, so the idle run counts what it starts.
calls="$work/calls"
# A real jq binary: a mise shim would find the shim below on PATH and loop.
real_jq=""
for j in $(type -ap jq); do
  case "$j" in */mise/shims/*) ;; *) real_jq="$j"; break ;; esac
done
[ -n "$real_jq" ] || real_jq="$(mise which jq 2>/dev/null)" || { echo "no jq binary found" >&2; exit 2; }
printf '#!/bin/sh\necho "jq $(ps -o command= -p $PPID | grep -o "scripts/[a-z-]*.sh *[a-z-]*" | head -n 1)" >>%s\nexec %s "$@"\n' "$calls" "$real_jq" >"$work/shims/jq"
printf '#!/bin/sh\necho "muxa-cli $1" >>%s\nexec %s "$@"\n' "$calls" "$BIN/muxa-cli" >"$work/shims/muxa-cli"
# Scripts start through `#!/usr/bin/env bash`, so this counts each by name.
real_bash="$(command -v bash)"
printf '#!/bin/sh\necho "bash ${1##*/} $2" >>%s\nexec %s "$@"\n' "$calls" "$real_bash" >"$work/shims/bash"
chmod +x "$work/shims/jq" "$work/shims/muxa-cli" "$work/shims/bash"

ms() { perl -MTime::HiRes=time -e 'printf "%.0f", time * 1000'; }

# Mean wall time in ms of <runs> runs of a command, output discarded.
timed() {
  local runs="$1" s e i
  shift
  s="$(ms)"
  for ((i = 0; i < runs; i++)); do "$@" >/dev/null 2>&1 || true; done
  e="$(ms)"
  echo $(((e - s) / runs))
}

row() { printf '%-44s %8s\n' "$1" "$2"; }

mt new-session -d -s base -x 200 -y 50 -c "$work"
while [ "$(mt list-sessions | wc -l)" -lt "$sessions" ]; do mt new-session -d -c "$work"; done

echo "# Inbox bench: $sessions sessions, state copied from $state"
row "measure" "ms"
cli() { "$BIN/muxa-cli" --bridge "$bridge" "$@"; }
row "M1 muxa-cli inbox --json (one fetch)" "$(timed 5 cli inbox --json)"
row "M1a muxa-cli routines --json (bridge start)" "$(timed 5 cli routines --json)"
row "M1b muxa-cli list (sessions List)" "$(timed 5 cli list)"
cli inbox --json >"$work/inbox.json" 2>/dev/null
[ -n "$json" ] || json="$work/inbox.json"
row "  inbox json bytes" "$(wc -c <"$json" | tr -d ' ')"

export TMUX_AGENTS_SOCKET="$MUXA_SOCKET" AGENTS_SCRIPTS="$ROOT/tmux/scripts"
I="$ROOT/tmux/scripts/inbox.sh"
id="$(jq -r '[.items[] | select(.state == "open")][0].id // empty' "$json")"
printf '%s\0371\037keep\n' "$id" >"$work/state"
row "M2 inbox.sh view (a preview rendered alone)" "$(timed 10 "$I" view "$id" "$json")"
"$I" list "$json" >"$work/rows"
row "M2a inbox.sh render (every preview, per reload)" "$(timed 5 "$I" render "$json" "$work/rows" "$work/previews")"
row "M2b popup preview of a cursor move (cat)" "$(timed 10 sh -c 'cat "$1"/"$2" 2>/dev/null' x "$work/previews" "$id")"
row "M3 inbox.sh list" "$(timed 10 "$I" list "$json")"
row "M3 inbox.sh place (after each reload)" "$(timed 10 "$I" place "$work/state" "$json")"
row "M3 inbox.sh key (each key)" "$(timed 10 "$I" key "$id" d "$json")"

# Idle: the plugin with a sidebar per window, nothing happening.
mt set-environment -g PATH "$work/shims:$PATH"
mt set-environment -g XDG_STATE_HOME "$work/xdg"
mt set-environment -g TMPDIR "$work/tmp"
mt set-option -g @agents_check_gh off
mt set-option -g @agents_notify off
mt set-option -g @agents_muxa auto
mt set-option -g @agents_muxa_cli "$work/shims/muxa-cli"
mt set-option -g @agents_muxa_bridge "$bridge"
PATH="$work/shims:$PATH" TMPDIR="$work/tmp" XDG_STATE_HOME="$work/xdg" "$ROOT/tmux/agents.tmux"
sleep 5
sidebars="$(mt list-panes -a -F '#{@agents_sidebar_pane}' | grep -c '^1$' || true)"
: >"$calls"
server="$(mt display-message -p '#{pid}')"
cpu() { ps -o time= -p "$1" | awk -F'[:.]' '{ print ($1 * 60 + $2) * 100 + $3 }'; }
c0="$(cpu "$server")"
sleep "$idle"
c1="$(cpu "$server")"
echo
echo "# Idle for ${idle}s with $sidebars sidebars"
row "muxa-cli inbox calls" "$(grep -c '^muxa-cli inbox' "$calls" || true)"
row "muxa-cli calls, all" "$(grep -c '^muxa-cli' "$calls" || true)"
row "jq calls" "$(grep -c '^jq' "$calls" || true)"
row "script starts" "$(grep -c '^bash' "$calls" || true)"
echo
echo "# Starts by caller"
sort "$calls" | uniq -c | sort -rn | head -n 12
row "tmux server CPU (ms)" "$(((c1 - c0) * 10))"
[ -z "${BENCH_CALLS:-}" ] || cp "$calls" "$BENCH_CALLS"
