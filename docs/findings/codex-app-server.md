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

## Threads

ADR 0028's order (revision 1) relies on these. "Empty" means no turn and no rollout file. "Held" means a proxy connection that started the thread is still open. Rows marked *probe+TUI* ran the TUI in a `tmux -L muxa-test-codex` pane, with a model provider in the temporary home that needs no login and points at a closed port on 127.0.0.1. Nothing reached a model.

| Fact | Source |
|---|---|
| An empty thread read `idle` on a new connection right after `thread/start`. About 10 s later, `codex resume <id>` in a pane ended at once (the message was not captured). About 20 s after creation, `turn/start` got -32600 `thread not found: <id>`, and `thread/read` got -32600 `thread not loaded: <id>` twice, 15 s apart. The file at `thread.path` did not exist. | *login* |
| An empty thread that is not held reads `idle` up to 55 s after creation, and is gone (`thread not loaded: <id>`) at 60 to 65 s. Reads from other connections do not extend it. A failed `codex resume` does not shorten it. The *login* run saw it gone by 20 s, so the delay is not fixed. | probe |
| A held empty thread reads `idle` at 70 s and 140 s. It is gone 50 to 70 s after the holding connection closes. | probe |
| `codex resume <id>` on an empty thread exits 1 at once, held or not: `thread/resume failed during TUI bootstrap: thread/resume failed: no rollout found for thread id <id> (code -32600)`. `thread/resume` over the App Server answers the same -32600 error. Without a login the TUI stops at its sign-in screen first. | probe+TUI |
| `thread/start` takes no input. The schema (`codex app-server generate-json-schema --experimental`) lists config, instructions, `cwd`, `ephemeral`, `historyMode`, model, sandbox and source fields only. | probe |
| `thread/name/set` on an empty thread writes the rollout's first line, `session_meta`. `thread/metadata/update` writes two lines. `thread/inject_items` writes model-visible items. `thread/settings/update` writes nothing. `thread/list` stays empty after each. | probe |
| After `thread/name/set`, `codex resume <id>` opens the empty thread and shows the name. While the TUI runs, the thread reads `idle` (checked at 5, 30, 60 and 90 s), and `turn/start` on a new connection runs in it. The TUI shows the turn. | probe+TUI |
| A saved thread (it has a rollout) that nothing holds reads `notLoaded`, not an error. `turn/start` on it gets -32600 `thread not found: <id>`. `thread/resume` loads it again, and `turn/start` then works. | probe |
| The first `turn/start` writes the rollout within 1 s, before any model answer. `codex resume` then opens the thread. A turn whose model request fails stays `active` and keeps the thread loaded. | probe+TUI |
| A held connection receives `thread/started`, `thread/status/changed`, `turn/started`, `item/started`, `item/completed`, `warning` and `error` notifications for its thread. | probe |

`muxa-bridge codex-rpc --hold SECONDS` keeps a connection open after the answer and prints each notification's method and ids, so the *login* rows can be re-checked by hand (ADR 0028, developer checklist). `scripts/codex-app-server-probe.sh` re-checks `thread/resume` and `thread/name/set`. With `MUXA_PROBE_TUI=1` it also runs `codex resume` on a throwaway tmux server, and with `MUXA_PROBE_UNLOAD=1` it re-checks the unload rows (about 3 minutes).
