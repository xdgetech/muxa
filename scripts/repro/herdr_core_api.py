"""Checks herdr-core's framed API through `hc connect` (docs/findings/session-backends.md).

Starts one labelled terminal, attaches, types, resizes, reads its OSC 7501
status and events, then kills and removes it. Touches no other terminal.
Python 3.9 standard library only.
"""
import json
import os
import queue
import struct
import subprocess
import sys
import threading
import time

CWD = sys.argv[1]
NAME = "muxa-repro-%d" % os.getpid()
LABELS = {"muxa": "repro-%d" % os.getpid()}
SCRIPT = r"""
printf '\033]7501;state=working:app=muxa-repro\033\\'
echo ready
while read l; do
  echo "got:$l size:$(stty size)"
  [ "$l" = done ] && printf '\033]7501;state=done:app=muxa-repro\033\\'
done
"""
DATA, CREDIT, CLOSE, RESIZE = 0, 1, 2, 3

proc = subprocess.Popen(["hc", "connect"], stdin=subprocess.PIPE, stdout=subprocess.PIPE)
frames = queue.Queue()
replies = {}
streams = {}
next_id = [1]
failures = []


def check(ok, fact):
    print(("PASS  " if ok else "FAIL  ") + fact)
    if not ok:
        failures.append(fact)


def send_frame(channel, kind, payload):
    proc.stdin.write(struct.pack("<IBI", channel, kind, len(payload)) + payload)
    proc.stdin.flush()


def reader():
    while True:
        head = proc.stdout.read(9)
        if len(head) < 9:
            frames.put(None)
            return
        channel, kind, size = struct.unpack("<IBI", head)
        frames.put((channel, kind, proc.stdout.read(size)))


def pump(wait):
    try:
        frame = frames.get(timeout=wait)
    except queue.Empty:
        return
    if frame is None:
        raise SystemExit("FAIL  hc connect closed the connection")
    channel, kind, data = frame
    if channel == 0 and kind == DATA:
        msg = json.loads(data)
        if msg.get("id") is not None:
            replies[msg["id"]] = msg
        return
    stream = streams.setdefault(channel, {"bytes": bytearray(), "close": None})
    if kind == DATA:
        stream["bytes"] += data
        send_frame(channel, CREDIT, struct.pack("<I", len(data)))
    elif kind == CLOSE:
        stream["close"] = json.loads(data or b"{}")


def drain(seconds):
    end = time.time() + seconds
    while time.time() < end:
        pump(0.05)


def call(method, params, timeout=10.0):
    rid = next_id[0]
    next_id[0] += 1
    body = {"jsonrpc": "2.0", "id": rid, "method": method, "params": params}
    send_frame(0, DATA, json.dumps(body).encode())
    end = time.time() + timeout
    while time.time() < end:
        pump(0.05)
        if rid in replies:
            reply = replies.pop(rid)
            if "error" in reply:
                raise SystemExit("FAIL  %s: %s" % (method, reply["error"]))
            return reply["result"]
    raise SystemExit("FAIL  %s timed out" % method)


def wait_for(channel, needle, seconds=5.0):
    end = time.time() + seconds
    while needle not in streams.get(channel, {}).get("bytes", b"") and time.time() < end:
        pump(0.01)
    return needle in streams.get(channel, {}).get("bytes", b"")


def main():
    hello = {"jsonrpc": "2.0", "id": 0, "method": "hello", "params": {"generation": 1, "framed": True}}
    proc.stdin.write((json.dumps(hello) + "\n").encode())
    proc.stdin.flush()
    result = json.loads(proc.stdout.readline()).get("result", {})
    print("hc %s" % result.get("version"))
    check("attach" in result.get("features", []) and "events" in result.get("features", []),
          "hello advertises attach and events")
    threading.Thread(target=reader, daemon=True).start()

    sub = call("events.subscribe", {"labels": LABELS})
    call("terminal.run", {"argv": ["sh", "-c", SCRIPT], "name": NAME, "cwd": CWD,
                          "labels": LABELS, "no_parent": True, "no_notify": True})
    try:
        drain(0.5)
        att = call("terminal.attach", {"terminal": NAME, "cols": 100, "rows": 30, "history": 50})
        ch = att["channel"]
        check(wait_for(ch, b"ready"), "attach sends a first paint with the current screen")
        check(b"\x1b]7501;state=working" in bytes(streams[ch]["bytes"]),
              "the first paint carries the latest OSC 7501 report")

        send_frame(ch, DATA, b"one\r")
        check(wait_for(ch, b"got:one size:30 100"), "input reaches the program; attach's size is its size")

        send_frame(ch, RESIZE, struct.pack("<HH", 120, 40))
        drain(0.3)
        send_frame(ch, DATA, b"two\r")
        check(wait_for(ch, b"got:two size:40 120"), "a RESIZE frame reaches the program")

        send_frame(ch, DATA, b"done\r")
        drain(0.5)
        status = call("terminal.get", {"terminal": NAME}).get("status") or {}
        check(status.get("state") == "done" and status.get("app") == "muxa-repro",
              "terminal.get reports the program's OSC 7501 state")

        call("terminal.kill", {"terminal": NAME})
        drain(1.0)
        close = streams[ch]["close"] or {}
        check(close.get("reason") == "killed", "kill closes the stream with reason killed")

        events = bytes(streams.get(sub["channel"], {}).get("bytes", b"")).decode(errors="replace")
        types = [json.loads(line).get("params", {}).get("type") for line in events.splitlines() if line.strip()]
        check(types[:1] == ["created"] and "status" in types and "size" in types,
              "events start with created, then status and size follow (%s)" % ",".join(types))

        removed = call("terminal.rm", {"terminal": NAME})
        check(bool(removed.get("removed")), "terminal.rm removes the finished terminal")
    finally:
        try:
            call("terminal.kill", {"terminal": NAME}, timeout=3)
        except SystemExit:
            pass
        try:
            call("terminal.rm", {"terminal": NAME}, timeout=3)
        except SystemExit:
            pass
        proc.stdin.close()
        proc.terminate()
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
