#!/usr/bin/env bash
# Fact: the menu Claude Code shows when a session started with
# --permission-mode plan calls ExitPlanMode, and what pressing the digit of
# "Yes, manually approve edits" does. No model runs: a fake Messages API on
# 127.0.0.1 answers with a Write of the plan file, then ExitPlanMode.
# Uses a throwaway CLAUDE_CONFIG_DIR and a dummy API key, never ~/.claude.json.
# Runs only with MUXA_REPRO_CLAUDE=1: CI has no Claude Code.
set -euo pipefail
: "${MUXA_SOCKET:=muxa-repro-$$}"
. "$(dirname "$0")/../lib/tmux.bash"

if [ "${MUXA_REPRO_CLAUDE:-}" != 1 ] || ! command -v claude >/dev/null || ! command -v python3 >/dev/null; then
  printf 'SKIP  set MUXA_REPRO_CLAUDE=1, with claude and python3 on PATH\n'
  exit 0
fi
base="$(cd "$(mktemp -d "${TMPDIR:-/tmp}/muxa-plan.XXXXXX")" && pwd -P)"
server=
trap '[ -z "$server" ] || kill "$server" 2>/dev/null; mt_down; rm -rf "$base"' EXIT
mkdir -p "$base/cfg" "$base/root" "$base/bin"
key=sk-ant-api03-muxa-probe-0000000000000000000000
printf '{"hasCompletedOnboarding":true,"customApiKeyResponses":{"approved":["%s"],"rejected":[]},"projects":{"%s":{"hasTrustDialogAccepted":true}}}\n' \
  "${key: -20}" "$base/root" >"$base/cfg/.claude.json"
printf 'claude %s\n' "$(claude --version)"

# The fake API. Turn 1: Write the plan file the request names. Turn 2:
# ExitPlanMode. A prompt "muxa-probe-edit": a Write in the root. A prompt
# "muxa-probe-feedback": logged. Any other request (title, summary): a text.
cat >"$base/bin/fake.py" <<'EOF'
import json, re, sys
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

def sse(ev, data):
    return ("event: %s\ndata: %s\n\n" % (ev, json.dumps(data))).encode()

def answer(body):
    names = [t.get("name") for t in body.get("tools") or []]
    text = json.dumps(body)
    with open(sys.argv[2], "a") as f:
        f.write("tools=%s plans=%s\n" % (names, re.findall(r'[^\s"\\`]*/plans/[^\s"\\`]*', text)[:3]))
    if "muxa-probe" not in text or "Write" not in names:
        return [{"type": "text", "text": "Plan menu probe"}], "end_turn"
    used = re.findall(r'"tool_use_id": "(toolu_[a-z]+)"', text)
    if "muxa-probe-feedback" in text:
        with open(sys.argv[2], "a") as f:
            f.write("prompt muxa-probe-feedback\n")
        return [{"type": "text", "text": "muxa-probe-feedback-seen"}], "end_turn"
    if "muxa-probe-edit" in text and "toolu_edit" not in used:
        return [{"type": "tool_use", "id": "toolu_edit", "name": "Write", "input": {
            "file_path": sys.argv[3] + "/probe.txt", "content": "muxa\n"}}], "tool_use"
    if "toolu_write" not in used:
        m = re.search(r'(/[^\s"\\`]*/plans/[^\s"\\`]+\.md)', text)
        path = m.group(1) if m else "/nonexistent/plans/plan.md"
        return [{"type": "tool_use", "id": "toolu_write", "name": "Write", "input": {
            "file_path": path, "content": "# Probe plan\n\nmuxa-probe-plan: change nothing.\n"}}], "tool_use"
    if "toolu_exit" not in used:
        return [{"type": "tool_use", "id": "toolu_exit", "name": "ExitPlanMode", "input": {}}], "tool_use"
    return [{"type": "text", "text": "muxa-probe-done"}], "end_turn"

class Handler(BaseHTTPRequestHandler):
    def log_message(self, *args):
        pass

    def send(self, ctype, data):
        self.send_response(200)
        self.send_header("content-type", ctype)
        self.send_header("content-length", str(len(data)))
        self.end_headers()
        self.wfile.write(data)

    def do_GET(self):
        self.send("application/json", b"{}")

    def do_POST(self):
        size = int(self.headers.get("content-length") or 0)
        body = json.loads(self.rfile.read(size) or b"{}")
        if not self.path.startswith("/v1/messages") or "count_tokens" in self.path:
            self.send("application/json", b'{"input_tokens":10}')
            return
        content, stop = answer(body)
        msg = {"id": "msg_probe", "type": "message", "role": "assistant",
               "model": body.get("model", "claude"), "content": content,
               "stop_reason": stop, "stop_sequence": None,
               "usage": {"input_tokens": 10, "output_tokens": 5}}
        if not body.get("stream"):
            self.send("application/json", json.dumps(msg).encode())
            return
        out = [sse("message_start", {"type": "message_start",
                                     "message": dict(msg, content=[], stop_reason=None)})]
        for i, block in enumerate(content):
            if block["type"] == "text":
                start = {"type": "text", "text": ""}
                delta = {"type": "text_delta", "text": block["text"]}
            else:
                start = dict(block, input={})
                delta = {"type": "input_json_delta", "partial_json": json.dumps(block["input"])}
            out.append(sse("content_block_start", {"type": "content_block_start", "index": i, "content_block": start}))
            out.append(sse("content_block_delta", {"type": "content_block_delta", "index": i, "delta": delta}))
            out.append(sse("content_block_stop", {"type": "content_block_stop", "index": i}))
        out.append(sse("message_delta", {"type": "message_delta",
                                         "delta": {"stop_reason": stop, "stop_sequence": None},
                                         "usage": {"output_tokens": 5}}))
        out.append(sse("message_stop", {"type": "message_stop"}))
        self.send("text/event-stream", b"".join(out))

srv = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
with open(sys.argv[1] + ".tmp", "w") as f:
    f.write(str(srv.server_address[1]))
import os
os.rename(sys.argv[1] + ".tmp", sys.argv[1])
srv.serve_forever()
EOF
python3 -I "$base/bin/fake.py" "$base/port" "$base/requests.log" "$base/root" &
server=$!
wait_for "test -s '$base/port'"
port="$(cat "$base/port")"

# The pane's screen, joined, without trailing blanks.
screen() { mt capture-pane -p -J -t "$pane" | sed 's/[[:space:]]*$//'; }
# The footer's mode line.
footer() { screen | grep -E 'mode on|for shortcuts' | tail -n 1 | sed 's/^ *//'; }

# Starts claude --permission-mode plan in a new pane ($pane), sends one
# prompt and waits for the ExitPlanMode menu.
start_to_menu() {
  pane="$(mt new-session -d -P -F '#{pane_id}' -x 120 -y 40 -c "$base/root" \
    "env CLAUDE_CONFIG_DIR='$base/cfg' ANTHROPIC_BASE_URL='http://127.0.0.1:$port' ANTHROPIC_API_KEY='$key' claude --permission-mode plan")"
  wait_for "mt capture-pane -p -t '$pane' | grep -q 'plan mode on'" \
    || fail "the plan-mode session never shows 'plan mode on'; screen: $(screen)"
  prompt 'muxa-probe: plan nothing'
  wait_for "mt capture-pane -p -J -t '$pane' | grep -q 'manually approve edits'" \
    || fail "ExitPlanMode never shows the menu; requests: $(cat "$base/requests.log"); screen: $(screen)"
  sleep 1
}

# Types $1 as a prompt and presses Enter.
prompt() {
  mt send-keys -t "$pane" -l "$1"
  sleep 1
  mt send-keys -t "$pane" Enter
}

mt new-session -d -s hold \; set-option -g remain-on-exit on
start_to_menu
pass "a session started with --permission-mode plan shows 'plan mode on'"
printf 'menu screen:\n%s\n' "$(screen | grep -v '^$' | tail -n 16)"
menu="$(screen | grep -E '^[[:space:]]*(❯[[:space:]]*)?[0-9]+\. ')"
n="$(printf '%s\n' "$menu" | sed -n 's/^[^0-9]*\([0-9][0-9]*\)\. Yes, manually approve edits$/\1/p')"
[ -n "$n" ] || fail "no line '<n>. Yes, manually approve edits' in the menu: $menu"
pass "the ExitPlanMode menu has '$n. Yes, manually approve edits'"

mt send-keys -t "$pane" -l "$n"
wait_for "! mt capture-pane -p -J -t '$pane' | grep -q 'manually approve edits'" \
  || fail "pressing $n alone leaves the menu up; screen: $(screen)"
pass "pressing $n alone, without Enter, picks it and closes the menu"
wait_for "mt capture-pane -p -J -t '$pane' | grep -q muxa-probe-done" \
  || fail "the turn does not go on after '$n'; screen: $(screen)"
case "$(footer)" in
  *"manual mode on"*) pass "after '$n' the footer says: $(footer)" ;;
  *) fail "after '$n' the footer is not 'manual mode on': $(footer)" ;;
esac
prompt muxa-probe-edit
wait_for "mt capture-pane -p -J -t '$pane' | grep -q 'Do you want to'" \
  || fail "a Write after '$n' asks nothing; screen: $(screen)"
[ ! -e "$base/root/probe.txt" ] || fail "the Write ran before it was approved"
pass "after '$n' a Write asks first: $(screen | grep -m 1 'Do you want to' | sed 's/^ *//')"
mt kill-session -t "$pane"

# Escape on the menu: the turn ends, the mode stays plan, and the next text
# is a new prompt.
start_to_menu
mt send-keys -t "$pane" Escape
wait_for "! mt capture-pane -p -J -t '$pane' | grep -q 'manually approve edits'" \
  || fail "Escape leaves the menu up; screen: $(screen)"
sleep 2
case "$(footer)" in
  *"plan mode on"*) pass "Escape closes the menu and the mode stays plan: $(footer)" ;;
  *) fail "after Escape the footer is not 'plan mode on': $(footer)" ;;
esac
printf 'after Escape:\n%s\n' "$(screen | grep -v '^$' | tail -n 8)"
printf 'sessions file: %s\n' "$(cat "$base/cfg/sessions/"*.json 2>/dev/null || true)"
prompt muxa-probe-feedback
wait_for "grep -q 'prompt muxa-probe-feedback' '$base/requests.log'" \
  || fail "text typed after Escape sent no new request; screen: $(screen)"
wait_for "mt capture-pane -p -J -t '$pane' | grep -q muxa-probe-feedback-seen" \
  || fail "the answer to the text typed after Escape is not shown; screen: $(screen)"
pass "text typed after Escape is sent as a new prompt and answered"
