#!/usr/bin/env bash
# Re-checks docs/findings/codex-app-server.md on the installed codex: a private
# daemon in a temporary CODEX_HOME, no login, no model call, no ~/.codex.
# Not part of `make repro`: it needs codex and installs a daemon copy in the home.
# MUXA_PROBE_UNLOAD=1 also checks when an empty thread is dropped (about 3 min).
# MUXA_PROBE_TUI=1 also runs `codex resume` on a throwaway tmux server. The
# home then names a model provider that needs no login, on a closed local port.
set -euo pipefail
cd "$(dirname "$0")/.."
MUXA_SOCKET=muxa-test-codex-probe
# shellcheck source=lib/tmux.bash
. scripts/lib/tmux.bash
command -v codex >/dev/null || { echo "codex not found" >&2; exit 1; }
cargo build -q -p muxa-bridge
bridge="$PWD/target/debug/muxa-bridge"
home="$(mktemp -d)"
export CODEX_HOME="$home"
cleanup() {
  [ "${MUXA_PROBE_TUI:-}" != 1 ] || mt_down
  codex app-server daemon stop >/dev/null 2>&1 || true
  # `daemon stop` leaves the pid-update-loop helper of this home running.
  pkill -f "$home/packages/" 2>/dev/null || true
  rm -rf "$home"
}
trap cleanup EXIT
fail() { echo "FAIL: $*" >&2; exit 1; }
pass() { echo "ok: $*"; }

codex --version
if [ "${MUXA_PROBE_TUI:-}" = 1 ]; then
  # Port 9 (discard) is closed on a normal host: no request leaves it.
  cat >"$home/config.toml" <<'TOML'
model = "probe-model"
model_provider = "probe"
[model_providers.probe]
name = "probe"
base_url = "http://127.0.0.1:9/v1"
wire_api = "responses"
requires_openai_auth = false
request_max_retries = 0
stream_max_retries = 0
TOML
fi
codex app-server daemon start >/dev/null 2>&1 || fail "daemon start"
pass "a daemon starts in a fresh CODEX_HOME with no login"

init='{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"clientInfo":{"name":"probe","version":"0"}}}'
raw="$({ printf '%s\n' "$init"; sleep 3; } | codex app-server proxy | wc -c | tr -d ' ')"
[ "$raw" = 0 ] || fail "raw JSON through the proxy got $raw bytes"
pass "raw JSON lines through codex app-server proxy get no answer"

repo="$home/repo"
mkdir -p "$repo"
started="$("$bridge" codex-rpc thread/start "{\"cwd\":\"$repo\",\"historyMode\":\"legacy\"}")" \
  || fail "thread/start through muxa's WebSocket client"
id="$(jq -r .thread.id <<<"$started")"
[ "$(jq -r .thread.status.type <<<"$started")" = idle ] || fail "a new thread is not idle"
[ "$(jq -r .thread.historyMode <<<"$started")" = legacy ] || fail "historyMode is not legacy"
case "$(jq -r .thread.path <<<"$started")" in *"$id"*) ;; *) fail "the path lacks the id" ;; esac
pass "thread/start over WebSocket returns $id, idle, legacy, a path with the id"

read="$("$bridge" codex-rpc thread/read "{\"threadId\":\"$id\"}")" || fail "thread/read"
[ "$(jq -r .thread.status.type <<<"$read")" = idle ] || fail "a second connection does not read it idle"
pass "thread/read on a second connection reads it idle"

rpc() { "$bridge" codex-rpc "$@"; }
new_thread() { rpc thread/start "{\"cwd\":\"$repo\",\"historyMode\":\"legacy\"}" | jq -r .thread.id; }
status() { rpc thread/read "{\"threadId\":\"$1\"}" 2>/dev/null | jq -r .thread.status.type; }

rpc thread/resume "{\"threadId\":\"$id\"}" >/dev/null 2>&1 && fail "an empty thread resumes"
pass "thread/resume of an empty thread fails: no rollout"
rpc thread/name/set "{\"threadId\":\"$id\",\"name\":\"muxa-probe-name\"}" >/dev/null || fail "thread/name/set"
path="$(jq -r .thread.path <<<"$started")"
[ "$(head -1 "$path" | jq -r .type)" = session_meta ] || fail "thread/name/set wrote no session_meta"
rpc thread/resume "{\"threadId\":\"$id\"}" >/dev/null || fail "thread/resume after thread/name/set"
pass "thread/name/set writes the rollout's session_meta; thread/resume then works"

if [ "${MUXA_PROBE_TUI:-}" = 1 ]; then
  empty="$(new_thread)"
  mt new-session -d -s probe -x 200 -y 50
  mt set-option -g remain-on-exit on
  mt new-window -d -t probe -n empty "cd '$repo' && CODEX_HOME='$home' codex resume $empty"
  mt new-window -d -t probe -n saved "cd '$repo' && CODEX_HOME='$home' codex resume $id"
  sleep 8
  [ "$(mt display -p -t probe:empty '#{pane_dead_status}')" = 1 ] || fail "codex resume of an empty thread did not exit 1"
  mt capture-pane -p -J -t probe:empty | grep -q "no rollout found for thread id $empty" \
    || fail "codex resume of an empty thread did not say: no rollout found"
  pass "codex resume of an empty thread exits 1: no rollout found"
  [ "$(mt display -p -t probe:saved '#{pane_dead}')" = 0 ] || fail "codex resume of a saved thread exited"
  mt capture-pane -p -t probe:saved | grep -q muxa-probe-name || fail "the TUI does not show the saved thread's name"
  pass "codex resume opens a saved empty thread"
  # It holds the thread; the unload checks want nothing to hold it.
  mt kill-window -t probe:saved
fi

if [ "${MUXA_PROBE_UNLOAD:-}" = 1 ]; then
  empty="$(new_thread)"
  rpc --hold 80 thread/start "{\"cwd\":\"$repo\",\"historyMode\":\"legacy\"}" >"$home/held.json" 2>/dev/null &
  held_pid=$!
  until jq -e .thread.id "$home/held.json" >/dev/null 2>&1; do sleep 0.2; done
  held="$(jq -r .thread.id "$home/held.json")"
  sleep 75
  [ "$(status "$empty")" = "" ] || fail "an empty thread with no connection is still there after 75 s"
  pass "an empty thread is gone 75 s after its connection closed"
  [ "$(status "$held")" = idle ] || fail "a held empty thread was dropped"
  pass "a held empty thread is still idle after 75 s"
  [ "$(status "$id")" = notLoaded ] || fail "a saved thread with no connection is not notLoaded"
  pass "a saved thread with no connection reads notLoaded"
  wait "$held_pid"
  sleep 75
  [ "$(status "$held")" = "" ] || fail "the held thread is still there 75 s after its connection closed"
  pass "the held thread is gone 75 s after its connection closed"
fi

codex app-server daemon stop >/dev/null 2>&1
codex app-server daemon start >/dev/null 2>&1
read="$("$bridge" codex-rpc thread/read "{\"threadId\":\"$id\"}")" || fail "thread/read after stop and start"
[ "$(jq -r .thread.status.type <<<"$read")" = notLoaded ] || fail "after stop and start it is not notLoaded"
pass "after daemon stop and start the thread reads notLoaded"
