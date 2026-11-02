"""Holds a real tmux client open on a pty of a fixed size, like a desktop terminal.

Usage: pty-client.py <socket> <session> <cols> <rows> <seconds> [<type at> | stdin]
With <type at>, it types "x" that many seconds after it started, as a person would.
With "stdin", it types "x" for each line read on its stdin, so a test decides when.
"""
import os
import pty
import select
import struct
import sys
import termios
import fcntl
import time


def main() -> None:
    sock, session = sys.argv[1], sys.argv[2]
    cols, rows, secs = int(sys.argv[3]), int(sys.argv[4]), float(sys.argv[5])
    pid, fd = pty.fork()
    if pid == 0:
        os.environ["TERM"] = "xterm-256color"
        os.environ.pop("TMUX", None)
        os.execvp("tmux", ["tmux", "-L", sock, "attach-session", "-t", session])
    # Set the size before tmux reads it, so the client attaches at cols x rows.
    fcntl.ioctl(fd, termios.TIOCSWINSZ, struct.pack("HHHH", rows, cols, 0, 0))
    start = time.time()
    end = start + secs
    on_stdin = len(sys.argv) > 6 and sys.argv[6] == "stdin"
    type_at = start + float(sys.argv[6]) if len(sys.argv) > 6 and not on_stdin else None
    watched = [fd, sys.stdin.fileno()] if on_stdin else [fd]
    while time.time() < end:
        if type_at is not None and time.time() >= type_at:
            os.write(fd, b"x")
            type_at = None
        ready, _, _ = select.select(watched, [], [], 0.2)
        if on_stdin and sys.stdin.fileno() in ready:
            if not sys.stdin.readline():
                break
            os.write(fd, b"x")
        if fd in ready:
            try:
                os.read(fd, 65536)
            except OSError:
                break
    try:
        os.kill(pid, 15)
    except ProcessLookupError:
        pass


if __name__ == "__main__":
    main()
