# 0028: muxa creates the Codex thread it delivers to

## Status

Proposed. It answers the open item of ADR 0027 section 2 and the Codex gate of ADR 0024 and 0026. Slices S1 to S7 are built behind `[codex] owned_threads` in `workspaces.toml`, which is off by default. Keep it off until the developer's checklist at the end passes with a real login. Only fakes (`tests/data/codex-fake.py`) and a no-login probe of Codex 0.167.0's transport (`scripts/codex-app-server-probe.sh`, facts 6 and 7) have checked the code.

## Context

- A guarded run gets automatic input only when the adapter proves that the session belongs to the run's process (ADR 0024, `task_records/observation.rs`). Claude Code proves it with `sessions/<pid>.json`.
- Codex records no PID. The adapter matches rollouts by cwd and time, so it reports `process_bound = false`. A guarded Codex run falls back to the fixed prompt in Terminal (`docs/agents.md`).
- `docs/findings/workers-w0.md` shows that an inventory and a time cutoff can select another Codex in the same cwd. Better heuristics do not fix this.
- Codex CLI 0.166.0 runs the TUI as a client of a shared app-server daemon (`daemon_auto_start` is on). `codex --no-daemon` opts out per run.
- A Routine run has no person to type the fallback prompt (ADR 0026).

Facts this ADR relies on:

1. Hooks run inside the shared daemon, with the environment of the pane that started the daemon ([herdr #4859](https://github.com/herdrdev/herdr/issues/4859)). An environment nonce in the pane does not reach the hook.
2. `SessionStart` runs on the thread's first turn, not at `thread/start` ([openai/codex #54353](https://github.com/openai/codex/issues/54353)). A receipt cannot prove readiness before the first prompt.
3. Non-managed hooks run only after the user trusts their hash ([Hooks](https://learn.chatgpt.com/docs/hooks)).
4. `thread/start` creates a thread without a turn. `turn/start` sends input to a thread by id. `thread/status/changed` and `thread/read` report `idle`, `active` (with `waitingOnApproval`), `systemError` or `notLoaded` ([App Server](https://learn.chatgpt.com/docs/app-server)).
5. A no-model probe on a private stdio app-server, with a temporary `CODEX_HOME` and no login: `thread/start` returned an id, status `idle` and a rollout `path` that contains the id. No file existed on disk yet, and `thread/list` was empty. The default history mode was `paginated`; `historyMode: "legacy"` was accepted (experimental opt-in).
6. Codex 0.167.0: `codex app-server proxy` relays raw bytes to the daemon's control socket, `$CODEX_HOME/app-server-control/app-server-control.sock`. That socket speaks WebSocket. Newline-delimited JSON-RPC through the proxy gets no answer, and the proxy exits 0. A WebSocket upgrade gets `101 Switching Protocols` with `x-codex-websocket-max-unfragmented-message-bytes: 16777216`. The private stdio server (`codex app-server`) keeps newline-delimited JSON ([findings](../findings/codex-app-server.md)).
7. Through the proxy with WebSocket, on a daemon in a temporary `CODEX_HOME` with no login: `thread/start` returns an id, `idle` and `legacy`, and a second connection reads the thread `idle`. After `daemon stop` and `daemon start` the thread reads `notLoaded`. After `daemon restart` an empty thread still read `idle`.

## Decision

### Options

| | A. `SessionStart` receipt with a nonce | B. Owned thread, input typed into the TUI | C. Owned thread, input through App Server |
|---|---|---|---|
| Ownership proof | the hook echoes a nonce muxa put in the pane's env | muxa created the thread; the pane runs `codex resume <id>` | muxa created the thread and sends every turn to that id |
| What can go wrong | the daemon's env carries another pane's nonce (fact 1); no receipt before the first turn (fact 2); a hook in the user's Codex config needs trust; only safe with `--no-daemon`, which is unverified | an empty thread is not on disk (fact 5), so `resume` may fail; `/new` in the TUI sends typed input to another thread | the App Server API is marked experimental; a daemon restart unloads the thread |
| Change cost | hook script, install and trust flow, `--no-daemon` in every launch, receipt store | a small App Server client, launch command, adapter lookup by id | B, plus `turn/start` as a second input path in the sender |
| muxa must verify | the receipt's PID is inside the pane, and its nonce, source and session are fresh | pane PID/start identity, and the TUI still shows the owned thread | the thread id on the run, and its status is `idle` |

### Choose C

1. Before dispatch, muxa calls `thread/start` through `codex app-server proxy`, with the run's cwd and the workspace's agent environment. It stores the returned thread id on the reserved run, in the same durable step as `prepared` (ADR 0024).
2. The pane runs `codex resume <thread-id>`. The TUI is a viewer of the thread. It is not the proof of ownership.
3. Readiness is `thread/read` status `idle` on the stored id. `notLoaded`, `systemError`, a missing field or any error is unknown. Unknown never gets input.
4. Delivery keeps ADR 0024's claim: persist `delivering`, then call `turn/start` on the stored id. A JSON-RPC result is a definite send. A lost connection or timeout is uncertain and is not retried.
5. State comes from the thread status, so an approval shows as `waiting`. Chat reads the rollout at the thread's `path`, found by id, not by cwd and time. The adapter reports `process_bound = true` only for a run with a stored thread id.
6. The Loop and a Routine run use the same `turn/start` path. A Routine run then needs no person at the pane.
7. Manual Codex sessions and runs without a stored id keep today's behavior and the fallback prompt.

C is chosen over B because the proof does not depend on what the TUI shows. It is chosen over A because A cannot prove anything before the first turn while the daemon is shared.

### The developer checks these by hand before it is enabled

Use a real login, a throwaway tmux server (`tmux -L muxa-test-codex`), a scratch repo and a disposable prompt. Record only the version, ids, statuses and pass or fail.

1. `thread/start` through `codex app-server proxy` on the logged-in daemon returns an id with status `idle`, and starts no turn.
2. `codex resume <id>` in a pane opens that thread before its first turn. If it fails, record the error and try `turn/start` first, then `resume`.
3. `turn/start` with the fixed prompt runs in that thread. The attached TUI shows it. `thread/read` moves from `active` to `idle`. The rollout at `path` appears and holds the turn.
4. A command that needs approval shows `active` with `waitingOnApproval`.
5. After `/new` in the TUI, `turn/start` on the stored id still reaches the owned thread, not the new one.
6. With a second Codex in the same cwd, its sessions never bind to the run.
7. With a workspace `CODEX_HOME`, the thread is created under that home, and the TUI finds it.
8. After `codex app-server daemon restart`, the stored thread reads as `notLoaded` or fails, and muxa sends no input.

If check 1, 3 or 5 fails with the installed version, keep the fallback. If only check 2 fails, C still works with `turn/start` before `resume`; ADR 0024's order then needs a short amendment.

### Slices

| Slice | What | Checked by |
|---|---|---|
| S1 | `agents/codex/app_server.rs`: one JSON-RPC connection per call through `codex app-server proxy`, in WebSocket messages (fact 6). `thread/start` (cwd, `legacy` history), `thread/read`, `turn/start`, typed errors. A turn whose answer never came is uncertain. | unit tests against `tests/data/codex-fake.py` |
| S2 | Launch. A background Codex run creates its thread before dispatch and stores it on the reserved run under the creation claim. The pane runs `codex resume <id>`. | unit tests |
| S3 | State and Chat by thread id. `process_bound` only with a stored id. | unit tests |
| S4 | Readiness and delivery. `idle` on the stored id is ready. `delivering` is persisted, then `turn/start`. An uncertain send is never retried. | unit tests |
| S5 | The host switch. Routine runs, delegates, staff members and the overall assistant may run Codex with it. The writer stays Claude Code. | unit tests |
| S6 | Through the daemon on a `-L` server with a fake `codex`. | `tests/codex_threads.rs` |
| S7 | `docs/agents.md`, `docs/plan.md`, this ADR's status and the developer checklist. | review |

All seven slices are built.

### Choices made while building

1. **One host switch.** `[codex] owned_threads = true` at the top of `workspaces.toml`, off by default. The checks are about the host's Codex install, not one workspace. Off, a Codex Routine, delegate or staff member is refused as before, and the message names the switch.
2. **Which runs.** Only background runs: Routine runs and assistant runs (a delegate, a staff member, the overall assistant's plans). Tasks, ask sessions and manual sessions keep today's behavior (Decision item 7).
3. **The writer stays Claude Code** (ADR 0052 E1), also with the switch on. It performs an approved external write, and a wrong turn there has an effect outside the host. Revisit after the checks pass.
4. **Not owned, so still refused:** a Codex command with `--no-daemon` or `--remote`, and a workspace `command` that is not plain words starting with a `codex` program (for example `exec codex` or `codex $FLAGS`).
5. **Transport.** Each call spawns `codex app-server proxy` with the run's `CODEX_HOME`, opens a WebSocket over its stdin and stdout (`agents/codex/proxy.rs`, `tungstenite`), sends `initialize` with `experimentalApi`, then one request. Each JSON-RPC message is one text message. A message over the limit in the 101 answer is not sent. The proxy, not the socket path, is the transport: it finds the socket of the run's `CODEX_HOME` itself. Before `thread/start`, muxa runs `codex app-server daemon start`, because the TUI that would start the daemon runs only after the thread exists. A thread starts with `cwd` (the run folder) and `historyMode: "legacy"`; the full workspace env goes to that one call.
6. **The record.** `codexThread {version, run, id, path, program, home}` is written under the creation claim before the pane exists. It counts only for the run it names. A damaged field, or one for another run, gives no state and no input.
7. **The pane** runs `codex resume <id> <worker args>`: the id comes first, so a flag cannot take it as its value. Worker args reach the viewer. Whether they reach turns that muxa starts is checklist item 9.
8. **State.** `idle` is idle, `active` is running, `active` with `waitingOnApproval` or `waitingOnUserInput` is waiting, `notLoaded` and `systemError` are unknown. The time comes from the rollout's last turn event only when it agrees with the status; else it is absent. A pane back at a shell has no agent.
9. **Delivery.** A fresh `thread/read` must say `idle` under the delivery claim. Then `delivering` is persisted, then `turn/start`. A result is sent. An error answer is not sent: the start wait ends with its not-sent note, and the Loop stops with `not-sent`. A lost connection or a timeout after the request is uncertain: the claim stays for recovery and nothing is sent again.
10. **Not covered yet:** `restart-agent` on an owned run. The new run has no thread, so it reads unknown and gets the Terminal fallback. Re-using or re-creating the thread on restart is a follow-up.

## Consequences

- Unblocked after the checks pass: automatic Codex continuation after a managed restart (ADR 0027 section 2), the Codex part of ADR 0024's "stronger Codex binding" gate and R16, and Codex workers for Routine runs (ADR 0026).
- The bridge gets a second input path for Codex: a turn sent by id, not typed keys. Claude keeps typed input.
- muxa depends on the shared daemon for guarded Codex runs. A worker command with `--no-daemon` or `--remote` cannot be guarded and keeps the fallback.
- The App Server API is experimental. Every call fails closed to unknown, and the fallback prompt stays as the safety net.
- The shared daemon keeps the environment of whoever started it. A workspace env other than `CODEX_HOME` may not reach the turn. This is true for any daemon-mode Codex today.
- New threads default to paginated history. Option C may request `legacy` so the rollout reader keeps working. Manual sessions may already use paginated history; the Codex Chat reader needs a separate check for that.
- Nothing copies authentication, adds credentials or edits the user's Codex config.

## Developer checklist

Run it with your real Codex login, before you set `owned_threads = true` on a host you use. Use a scratch repo, a disposable prompt and a throwaway tmux server; never the default server. Record only the Codex version, thread ids, statuses and pass or fail.

Setup, in a new shell. The checks go through muxa's own App Server client (`muxa-bridge codex-rpc`), so they test the code path that runs use: `codex app-server proxy`, a WebSocket over its stdio (fact 6), `initialize`, one request, then the connection closes. `scripts/codex-app-server-probe.sh` re-checks the no-login facts first, without your login.

```sh
codex --version                       # record it
cargo build -p muxa-bridge            # in the muxa checkout
export MUXA_BRIDGE="$PWD/target/debug/muxa-bridge"
rpc() { "$MUXA_BRIDGE" codex-rpc "$@"; }   # prints the result; an error answer exits 1
export CHECK=/tmp/muxa-codex-check
mkdir -p "$CHECK/repo" "$CHECK/root/.muxa" "$CHECK/state" && cd "$CHECK/repo"
```

1. `codex app-server daemon start`, then `rpc thread/start '{"cwd":"'"$PWD"'","historyMode":"legacy"}'`. Expected: the result has `thread.id`, `thread.status.type` `"idle"`, a `thread.path` that contains the id, and `thread.historyMode` `"legacy"`. Save the id: `export T=<id>`. Then `rpc thread/read '{"threadId":"'"$T"'"}'` on a new connection still says `idle`: no turn started.
2. `tmux -L muxa-test-codex new-session -d -s v "codex resume $T"; sleep 3; tmux -L muxa-test-codex capture-pane -p -t v`. Expected: the TUI shows the empty thread, not a picker or an error. On failure, record the error, do step 3 first, then this step.
3. `rpc turn/start '{"threadId":"'"$T"'","input":[{"type":"text","text":"Reply with the word ok."}]}'`. It returns after the answer and closes its connection, as muxa does. Then `rpc thread/read '{"threadId":"'"$T"'"}' | jq -c .thread.status` while the turn runs and after. Expected: a result with `turn`; `status` `active` then `idle`, so the turn goes on after the connection closed; the TUI (`capture-pane` again) shows the turn; the file at `thread.path` exists and has `task_started` and `task_complete` lines.
4. With a default approval policy, `rpc turn/start '{"threadId":"'"$T"'","input":[{"type":"text","text":"Run touch approval-check in this folder."}]}'`, then `rpc thread/read '{"threadId":"'"$T"'"}' | jq -c .thread.status`. Expected: `{"type":"active","activeFlags":["waitingOnApproval"]}`. Deny it in the TUI.
5. Type `/new` in the TUI (`tmux -L muxa-test-codex send-keys -t v /new Enter`), then repeat step 3 on `$T`. Expected: the turn lands in `$T` (its `path` grows), not in the TUI's new thread.
6. Start a second Codex in the same folder in another pane (`tmux -L muxa-test-codex new-window -d -t v codex`) and talk to it. Expected: `thread/read` on `$T` does not change, and its `path` does not grow.
7. Pick a workspace's Codex home, `export W=<that home>`. Repeat steps 1 to 3 with `CODEX_HOME="$W" codex app-server daemon start`, `rpc --codex-home "$W" ...` for every call, and `CODEX_HOME="$W" codex resume $T` in the pane. Expected: `thread.path` is under `$W`, and the TUI finds the thread.
8. `codex app-server daemon restart`, then `rpc thread/read '{"threadId":"'"$T"'"}'`. Expected: an error answer (exit 1), or `status` `notLoaded`; muxa treats both as unknown. Record what you see: without a login, an empty thread still read `idle` after `restart` (fact 7). If it reads `idle`, also run `codex app-server daemon stop`, `codex app-server daemon start` and read again. Expected then: `notLoaded`.
9. Start a thread, then `codex resume $T --sandbox read-only` (or your worker's flags), then step 3 with "Create a file named flag-check". Record whether the turn kept the viewer's flag. If it did not, set sandbox and approval for owned runs in that Codex home's `config.toml`.

Then muxa end to end, on the same throwaway server:

10. Write `$CHECK/workspaces.toml` with `[codex]` `owned_threads = true`, one workspace `w` with `root = "$CHECK/root"`, `[workspace.agent] command = "codex"`, `[workspace.routines] enabled = true`, and a Routine in `$CHECK/root/.muxa/routines.toml` as in `docs/examples/routines.toml`. Run `"$MUXA_BRIDGE" daemon run --socket muxa-test-codex --daemon-dir /tmp/mxd-codex --state-dir "$CHECK/state" --workspaces "$CHECK/workspaces.toml"`, and in another shell `muxa-cli --bridge "$MUXA_BRIDGE --socket muxa-test-codex --daemon-dir /tmp/mxd-codex --state-dir $CHECK/state --workspaces $CHECK/workspaces.toml" routine run w/<name>`. Expected: the run's record in `$CHECK/state` has `codexThread` with `run` 1; its pane runs `codex resume <that id>`; the start prompt arrives as one turn in that thread; the Routine publishes; `/tmp/mxd-codex/daemon.log` holds ids and no prompt or instruction text.

If 1, 3 or 5 fails, keep the switch off. If only 2 fails, amend the order (Decision, the last paragraph of the checks above) before turning it on.

Clean up: `tmux -L muxa-test-codex kill-server; rm -rf "$CHECK" /tmp/mxd-codex`.
