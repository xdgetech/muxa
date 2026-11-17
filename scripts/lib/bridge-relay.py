"""Serves muxa-bridge over TCP on 127.0.0.1 for the simulator's UI tests.

Each connection starts its own bridge: argv after `--` is its command line.
Prints the port, then relays bytes until the test run ends. DEBUG builds only
connect here (launch argument `-bridge-port`); it never listens beyond loopback.
"""
import socket
import subprocess
import sys
import threading


def pump(read, write, done):
    try:
        while True:
            chunk = read(65536)
            if not chunk:
                break
            write(chunk)
    except OSError:
        pass
    finally:
        done()


def serve(conn, command):
    bridge = subprocess.Popen(command, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL)

    def to_bridge(chunk):
        bridge.stdin.write(chunk)
        bridge.stdin.flush()

    def close_bridge():
        try:
            bridge.stdin.close()
        except OSError:
            pass

    threading.Thread(target=pump, args=(conn.recv, to_bridge, close_bridge), daemon=True).start()
    pump(bridge.stdout.read1, conn.sendall, lambda: None)
    conn.close()
    bridge.wait()


def main():
    command = sys.argv[sys.argv.index("--") + 1:]
    server = socket.socket()
    server.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    server.bind(("127.0.0.1", 0))
    server.listen()
    print(server.getsockname()[1], flush=True)
    while True:
        conn, _ = server.accept()
        threading.Thread(target=serve, args=(conn, command), daemon=True).start()


if __name__ == "__main__":
    main()
