# Holds a real tmux client open on a pty, so tests can send keys with send-keys -K.
# Usage: pty-client.py <socket> <session> <cols> <rows> <seconds>
import os, pty, sys, time, fcntl, termios, struct, select
sock, session, cols, rows, secs = sys.argv[1], sys.argv[2], int(sys.argv[3]), int(sys.argv[4]), float(sys.argv[5])
pid, fd = pty.fork()
if pid == 0:
    os.environ["TERM"] = "xterm-256color"
    os.execvp("tmux", ["tmux", "-L", sock, "attach", "-t", session])
fcntl.ioctl(fd, termios.TIOCSWINSZ, struct.pack("HHHH", rows, cols, 0, 0))
end = time.time() + secs
while time.time() < end:
    r, _, _ = select.select([fd], [], [], 0.2)
    if r:
        try: os.read(fd, 65536)
        except OSError: break
