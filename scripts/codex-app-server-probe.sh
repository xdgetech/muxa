#!/usr/bin/env bash
# Re-checks docs/findings/codex-app-server.md on the installed codex: a private
# daemon in a temporary CODEX_HOME, no login, no model call, no ~/.codex.
# Not part of `make repro`: it needs codex and installs a daemon copy in the home.
set -euo pipefail
cd "$(dirname "$0")/.."
command -v codex >/dev/null || { echo "codex not found" >&2; exit 1; }
cargo build -q -p muxa-bridge
bridge="$PWD/target/debug/muxa-bridge"
home="$(mktemp -d)"
export CODEX_HOME="$home"
cleanup() {
  codex app-server daemon stop >/dev/null 2>&1 || true
  # `daemon stop` leaves the pid-update-loop helper of this home running.
  pkill -f "$home/packages/" 2>/dev/null || true
  rm -rf "$home"
}
trap cleanup EXIT
fail() { echo "FAIL: $*" >&2; exit 1; }
pass() { echo "ok: $*"; }

codex --version
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

codex app-server daemon stop >/dev/null 2>&1
codex app-server daemon start >/dev/null 2>&1
read="$("$bridge" codex-rpc thread/read "{\"threadId\":\"$id\"}")" || fail "thread/read after stop and start"
[ "$(jq -r .thread.status.type <<<"$read")" = notLoaded ] || fail "after stop and start it is not notLoaded"
pass "after daemon stop and start the thread reads notLoaded"
