# Codex App Server: verified transport facts

ADR 0028's client (`crates/muxa-bridge/src/agents/codex/app_server.rs`, `proxy.rs`) relies on these facts. Checked on Codex CLI 0.167.0, macOS. Rows marked *login* come from the developer's real login; the others come from `scripts/codex-app-server-probe.sh`, which uses a temporary `CODEX_HOME`, no login and no model call. Re-run the script after a Codex upgrade.

## Transport

| Fact | Source |
|---|---|
| `codex app-server` (no subcommand) is a private server on stdio. It answers newline-delimited JSON-RPC: `initialize` gives `userAgent` and `codexHome`; `thread/start {cwd, historyMode: "legacy"}` gives `thread.id`, followed by notifications such as `thread/started`, `mcpServer/startupStatus/updated`, `account/updated` and `remoteControl/status/changed`. | *login*; ADR 0028 fact 5 |
| `codex app-server proxy` ("Proxy stdio bytes to the running app-server control socket"; `--sock`, `-c`, `--enable`, `--disable`) answers nothing to newline-delimited JSON-RPC, with or without `"jsonrpc":"2.0"`, with stdin open 15 s and `RUST_LOG=debug`. It exits 0. | *login*; probe |
| The control socket is `$CODEX_HOME/app-server-control/app-server-control.sock`, a Unix socket. With the default home it is a symlink to `/private/tmp/codex-daemon-<uid>/<hash>`. `codex app-server daemon version` prints JSON with `status`, `backend`, `managedCodexPath` and `socketPath`. | *login*; probe |
| A plain HTTP GET on the socket gets no answer. A WebSocket upgrade (`Connection: Upgrade`, `Upgrade: websocket`, `Sec-WebSocket-Version: 13`, a key) gets `HTTP/1.1 101 Switching Protocols` with `x-codex-websocket-max-unfragmented-message-bytes: 16777216`. | *login*; probe |
| Through `proxy`, a WebSocket client works: the handshake to `ws://localhost/`, then one JSON-RPC message per text frame. `initialize`, `thread/start` and `thread/read` answer, with notifications between the answers. So the proxy relays raw bytes, and the client does the WebSocket handshake and framing itself. | probe |

## Daemon

| Fact | Source |
|---|---|
| `codex app-server daemon start` works in a fresh `CODEX_HOME` with no login. It copies the CLI into `$CODEX_HOME/packages/app-server-daemon` and prints progress lines and a JSON status. | probe |
| A thread made on one proxy connection reads `idle` on the next one. muxa opens one connection per call. | probe |
| An empty thread (no turn) reads `notLoaded` after `daemon stop` and `daemon start`. After `daemon restart` (a new daemon PID) it read `idle`, twice in a row, also 3 s later. | probe (by hand for `restart`) |
| `daemon stop` leaves the home's `codex app-server daemon pid-update-loop` helper running. | probe |
| Without a daemon, the proxy closes before the 101 answer: muxa reports a handshake error. | probe |

`thread/start` through the daemon, without login: `thread.status.type` `idle`, `historyMode` `legacy`, and a `thread.path` under `$CODEX_HOME/sessions/` that contains the id. No file exists there before the first turn.
