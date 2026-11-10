# 0028: muxa creates the Codex thread it delivers to

## Status

Proposed. It answers the open item of ADR 0027 section 2 and the Codex gate of ADR 0024 and 0026. Slices S1 to S7 below are being implemented behind a host switch that is off by default. It needs the developer's checks below, with a real login, before it is enabled.

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
| S1 | `agents/codex/app_server.rs`: one JSON-RPC connection per call through `codex app-server proxy`. `thread/start` (cwd, `legacy` history), `thread/read`, `turn/start`, typed errors. A turn whose answer never came is uncertain. | unit tests against `tests/data/codex-fake.py` |
| S2 | Launch. A background Codex run creates its thread before dispatch and stores it on the reserved run under the creation claim. The pane runs `codex resume <id>`. | unit tests |
| S3 | State and Chat by thread id. `process_bound` only with a stored id. | unit tests |
| S4 | Readiness and delivery. `idle` on the stored id is ready. `delivering` is persisted, then `turn/start`. An uncertain send is never retried. | unit tests |
| S5 | The host switch. Routine runs, delegates, staff members and the overall assistant may run Codex with it. The writer stays Claude Code. | unit tests |
| S6 | Through the daemon on a `-L` server with a fake `codex`. | `tests/codex_threads.rs` |
| S7 | `docs/agents.md`, `docs/plan.md`, this ADR's status and the developer checklist. | review |

## Consequences

- Unblocked after the checks pass: automatic Codex continuation after a managed restart (ADR 0027 section 2), the Codex part of ADR 0024's "stronger Codex binding" gate and R16, and Codex workers for Routine runs (ADR 0026).
- The bridge gets a second input path for Codex: a turn sent by id, not typed keys. Claude keeps typed input.
- muxa depends on the shared daemon for guarded Codex runs. A worker command with `--no-daemon` or `--remote` cannot be guarded and keeps the fallback.
- The App Server API is experimental. Every call fails closed to unknown, and the fallback prompt stays as the safety net.
- The shared daemon keeps the environment of whoever started it. A workspace env other than `CODEX_HOME` may not reach the turn. This is true for any daemon-mode Codex today.
- New threads default to paginated history. Option C may request `legacy` so the rollout reader keeps working. Manual sessions may already use paginated history; the Codex Chat reader needs a separate check for that.
- Nothing copies authentication, adds credentials or edits the user's Codex config.
