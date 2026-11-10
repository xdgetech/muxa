#!/usr/bin/env python3
"""A stand-in `codex` for ADR 0028's tests. It never calls a model.

  codex app-server daemon start   records the call; exits 1 with fake/daemon-fails
  codex app-server                a private server: newline-delimited JSON-RPC on stdio
  codex app-server proxy          the shared daemon, as the real proxy relays it: a
                                  WebSocket handshake, then one JSON-RPC message per
                                  text frame. Raw JSON lines get no answer, as with
                                  Codex 0.167.0 (docs/findings/codex-app-server.md).
  codex resume <id> [args...]     a viewer: records its argv and every typed line
  codex fake-work <id>            one turn's work, started by turn/start

State lives under $CODEX_HOME/fake/, so every connection sees the same threads:
  threads/<id>.json  the thread: cwd, path, status
  starts.log         one JSON line per thread/start
  turns.log          "<id>\\t<text>" per turn/start that was answered or dropped
  resume.log         the viewer's argv
  typed.log          lines typed into the viewer (muxa must type none)
Steering files there:
  status-<id>        overrides the status: idle, active, approval, notLoaded,
                     systemError, missing (no status field) or bogus
  status             the same, for every thread without its own file
  drop-turn          turn/start is recorded, then the connection drops unanswered
  silent-turn        turn/start is recorded and never answered
  refuse-turn        turn/start gets a JSON-RPC error
  silent             no request is answered
  max-message        the byte limit the 101 answer names (default 16777216)
  turn-<n>.json      {"path": ..., "content": ...}: the n-th turn writes content
                     to path, relative to the thread's cwd
"""

import base64
import hashlib
import json
import os
import subprocess
import sys
import time
import uuid

HOME = os.environ.get("CODEX_HOME", "")
FAKE = os.path.join(HOME, "fake")


def path(*parts):
    return os.path.join(FAKE, *parts)


def append(name, line):
    os.makedirs(FAKE, exist_ok=True)
    with open(path(name), "a") as f:
        f.write(line + "\n")


def write_json(file, value):
    os.makedirs(os.path.dirname(file), exist_ok=True)
    tmp = file + ".tmp-%d" % os.getpid()
    with open(tmp, "w") as f:
        json.dump(value, f)
    os.replace(tmp, file)


def load(tid):
    try:
        with open(path("threads", tid + ".json")) as f:
            return json.load(f)
    except (OSError, ValueError):
        return None


def save(thread):
    write_json(path("threads", thread["id"] + ".json"), thread)


def status_of(thread):
    word = thread["status"]
    for name in ("status-" + thread["id"], "status"):
        try:
            with open(path(name)) as f:
                word = f.read().strip()
            break
        except OSError:
            continue
    if word == "approval":
        return {"type": "active", "activeFlags": ["waitingOnApproval"]}
    if word == "active":
        return {"type": "active", "activeFlags": []}
    if word == "missing":
        return None
    return {"type": word}


def shown(thread):
    out = {"id": thread["id"], "cwd": thread["cwd"], "path": thread["path"],
           "historyMode": thread.get("historyMode", "paginated")}
    status = status_of(thread)
    if status is not None:
        out["status"] = status
    return out


WS_GUID = "258EAFA5-E914-47DA-95CA-C5AB0DC85B11"
MAX_MESSAGE = 16 << 20
OUT = sys.stdout.buffer


def line_out(value):
    OUT.write(json.dumps(value).encode() + b"\n")
    OUT.flush()


send = line_out


def ws_frame(opcode, data):
    n = len(data)
    if n < 126:
        head = bytes([0x80 | opcode, n])
    elif n < 1 << 16:
        head = bytes([0x80 | opcode, 126]) + n.to_bytes(2, "big")
    else:
        head = bytes([0x80 | opcode, 127]) + n.to_bytes(8, "big")
    OUT.write(head + data)
    OUT.flush()


def ws_out(value):
    # A ping first, as a live server may: the client must answer and go on.
    ws_frame(0x9, b"fake")
    ws_frame(0x1, json.dumps(value).encode())


class Input:
    def __init__(self):
        self.buf = b""

    def fill(self):
        chunk = os.read(0, 65536)
        if not chunk:
            sys.exit(0)
        self.buf += chunk

    def take(self, n):
        while len(self.buf) < n:
            self.fill()
        out, self.buf = self.buf[:n], self.buf[n:]
        return out

    def head(self):
        while b"\r\n\r\n" not in self.buf:
            self.fill()
        head, self.buf = self.buf.split(b"\r\n\r\n", 1)
        return head


def handshake(stdin):
    """The upgrade answer, or None for anything that is not one."""
    lines = stdin.head().decode("latin-1").split("\r\n")
    if not lines[0].startswith("GET "):
        return None
    headers = {}
    for line in lines[1:]:
        name, _, value = line.partition(":")
        headers[name.strip().lower()] = value.strip()
    key = headers.get("sec-websocket-key")
    if headers.get("upgrade", "").lower() != "websocket" or not key:
        return None
    accept = base64.b64encode(hashlib.sha1((key + WS_GUID).encode()).digest()).decode()
    try:
        with open(path("max-message")) as f:
            limit = int(f.read().strip())
    except (OSError, ValueError):
        limit = MAX_MESSAGE
    return ("HTTP/1.1 101 Switching Protocols\r\nconnection: Upgrade\r\n"
            "upgrade: websocket\r\nsec-websocket-accept: %s\r\n"
            "x-codex-websocket-max-unfragmented-message-bytes: %d\r\n\r\n"
            % (accept, limit)).encode()


def ws_messages(stdin):
    """Text messages from masked client frames; answers pings, ends on close."""
    message = b""
    while True:
        b0, b1 = stdin.take(2)
        fin, opcode, n = b0 & 0x80, b0 & 0x0F, b1 & 0x7F
        if not b1 & 0x80:
            sys.exit(1)  # a client frame must be masked
        if n == 126:
            n = int.from_bytes(stdin.take(2), "big")
        elif n == 127:
            n = int.from_bytes(stdin.take(8), "big")
        mask = stdin.take(4)
        data = bytes(b ^ mask[i % 4] for i, b in enumerate(stdin.take(n)))
        if opcode == 0x8:
            ws_frame(0x8, b"")
            sys.exit(0)
        if opcode == 0x9:
            ws_frame(0xA, data)
            continue
        if opcode in (0x1, 0x0):
            message += data
            if fin:
                yield message.decode()
                message = b""


def error(rid, message):
    send({"id": rid, "error": {"code": -32600, "message": message}})


def stdio():
    for line in sys.stdin:
        handle(json.loads(line))


def proxy():
    global send
    stdin = Input()
    answer = handshake(stdin)
    if answer is None:
        # The real proxy relays these bytes to a WebSocket server, which
        # waits for a handshake that never comes: no answer, exit 0 at EOF.
        while True:
            stdin.fill()
    OUT.write(answer)
    OUT.flush()
    send = ws_out
    for text in ws_messages(stdin):
        handle(json.loads(text))


def handle(msg):
    rid, method = msg.get("id"), msg.get("method")
    params = msg.get("params") or {}
    if rid is None:
        return
    if os.path.exists(path("silent")):
        return
    # The real server sends notifications between answers.
    send({"method": "remoteControl/status/changed", "params": {"status": "disabled"}})
    if method == "initialize":
        send({"id": rid, "result": {"userAgent": "fake", "codexHome": HOME}})
    elif method == "thread/start":
        tid = str(uuid.uuid4())
        day = time.strftime("%Y/%m/%d", time.gmtime())
        stamp = time.strftime("%Y-%m-%dT%H-%M-%S", time.gmtime())
        rollout = os.path.join(HOME, "sessions", day, "rollout-%s-%s.jsonl" % (stamp, tid))
        thread = {"id": tid, "cwd": params.get("cwd"), "path": rollout, "status": "idle",
                  "historyMode": params.get("historyMode", "paginated"), "turns": 0}
        save(thread)
        append("starts.log", json.dumps({"cwd": params.get("cwd"),
                                         "historyMode": params.get("historyMode")}))
        send({"id": rid, "result": {"thread": shown(thread), "model": "fake"}})
    elif method == "thread/read":
        thread = load(params.get("threadId", ""))
        if thread is None:
            error(rid, "thread not loaded: %s" % params.get("threadId"))
        else:
            send({"id": rid, "result": {"thread": shown(thread)}})
    elif method == "turn/start":
        thread = load(params.get("threadId", ""))
        if thread is None:
            error(rid, "thread not loaded")
            return
        text = "".join(i.get("text", "") for i in params.get("input", []))
        if os.path.exists(path("refuse-turn")):
            error(rid, "turn refused")
            return
        append("turns.log", "%s\t%s" % (thread["id"], text))
        if os.path.exists(path("drop-turn")):
            sys.exit(0)
        if os.path.exists(path("silent-turn")):
            return
        thread["status"] = "active"
        thread["turns"] += 1
        save(thread)
        subprocess.Popen([sys.executable, os.path.abspath(__file__), "fake-work", thread["id"]],
                         stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL,
                         stderr=subprocess.DEVNULL, start_new_session=True)
        send({"id": rid, "result": {"turn": {"id": "turn-%d" % thread["turns"]}}})
    else:
        error(rid, "unknown method")


def event(kind, **fields):
    payload = {"type": kind}
    payload.update(fields)
    return {"type": "event_msg", "payload": payload}


def work(tid):
    time.sleep(0.5)
    thread = load(tid)
    n = thread["turns"]
    lines = []
    if not os.path.exists(thread["path"]):
        os.makedirs(os.path.dirname(thread["path"]), exist_ok=True)
        lines.append({"type": "session_meta", "payload": {"id": tid, "cwd": thread["cwd"]}})
    lines.append(event("task_started", started_at=int(time.time())))
    lines.append({"type": "response_item", "payload": {
        "type": "message", "id": "a%d" % n, "role": "assistant",
        "content": [{"type": "output_text", "text": "fake turn %d done" % n}]}})
    steer = path("turn-%d.json" % n)
    if os.path.exists(steer):
        with open(steer) as f:
            spec = json.load(f)
        write_json(os.path.join(thread["cwd"], spec["path"]), spec["content"])
    lines.append(event("task_complete", completed_at=int(time.time())))
    with open(thread["path"], "a") as f:
        for line in lines:
            f.write(json.dumps(line) + "\n")
    thread = load(tid)
    thread["status"] = "idle"
    save(thread)


def viewer(args):
    append("resume.log", " ".join(args))
    print("fake codex viewer", flush=True)
    for line in sys.stdin:
        append("typed.log", line.rstrip("\n"))
    while True:
        time.sleep(60)


def main():
    args = sys.argv[1:]
    if args[:3] == ["app-server", "daemon", "start"]:
        append("calls.log", "daemon-start")
        sys.exit(1 if os.path.exists(path("daemon-fails")) else 0)
    if args == ["app-server"]:
        stdio()
    elif args[:2] == ["app-server", "proxy"]:
        proxy()
    elif args[:1] == ["resume"]:
        viewer(args[1:])
    elif args[:1] == ["fake-work"]:
        work(args[1])
    else:
        sys.exit(2)


if __name__ == "__main__":
    main()
