# 0048: Captures, Work items and assistant runs (Personal Assistant PA0 and PA1)

## Status

Proposed on 2026-11-09. It implements stages PA0 and PA1 of ADR 0045 ([specification](../personal-assistant.md)). It changes ADR 0035 §5: `+` opens Capture on a host with `work:9`. It widens the Inbox screen of ADR 0033 without changing its items. Slices A0–A12 are implemented.

## Context

- A Task starts an agent (`create-task`, `protocol/PROTOCOL.md` "Task records"), and `list-tasks` lists every record. An unstarted idea cannot be a Task.
- An Inbox item is one question tied to one Task (`items/store.rs`: files keyed by Task id, one open item per Task, 50 kept). An unstarted idea cannot be an item either.
- A Routine run is a record of kind `routine-run` (ADR 0026 §3). It already has what a delegated request needs: a private folder, a fixed brief in `.muxa/TASK.md`, automatic prompt delivery with ADR 0024 claims, and an in-process check that validates and publishes a file.
- ADR 0045 leaves open: who coordinates, how requests and results travel, retention, the Inbox merge, budgets (spec §12).

## Decision

### 1. PA0 and PA1 have no coordinator process

The overall assistant of ADR 0045 starts in PA2. In PA0 and PA1, the user picks the workspace. Host configuration in `workspaces.toml`:

- `[assistant]`: `enabled`, `max_runs_per_day` across all workspaces.
- `[workspace.assistant]`: `delegate = true` to allow requests to this workspace; optional `worker`; `max_open_runs` (default 1), `max_runs_per_day` (default 10), `run_timeout` (default `"30m"`).

A workspace without `[workspace.assistant]` accepts captures but no requests.

### 2. Three new records in their own store

`<state dir>/assistant/` holds one JSON file per record, version 1, with the item store's lock and atomic write. Unknown fields are kept; an unknown version is refused.

| Record | File | Holds |
|---|---|---|
| Capture | `captures/c-<8hex>.json` | `clientId`, original text (≤ 16 KiB, never rewritten), source (`app`, `tmux`, `cli`), optional workspace, state `received` (unsorted) / `sorted`, linked Work items, `rev` |
| Work item | `work/w-<8hex>.json` | title, workspace, state `open` / `done` / `withdrawn`, `laterAt`, `mergedInto`, linked Captures, requests and Tasks, `rev`. `in-progress` and `waiting-decision` are derived from the linked Tasks and requests when listed. |
| Assistant request | `requests/q-<8hex>.json` | Work item, `version`, workspace, scope `research` / `compare` / `draft`, purpose, run (`t-…`), delivery state with its time, results (≤ 10 kept) |

User notes on a Capture, a due date and a stored next action come later; PA0 and PA1 do not need them.

A Capture and a Work item link many-to-many. Nothing merges or discards them without the user. Captures are never pruned automatically. Lists cap at 200 Captures and 100 closed Work items. Retention and audit are PA3.

### 3. Duplicates

- **Repeated actions.** A capture carries a client `clientId` (UUID); the same `clientId` returns the stored Capture and writes nothing. One `(work, rev)` makes one request. One `(request, version)` makes one run, found again after a crash by the goal line `muxa-request: q-… v<n>`. A time-limited `creating` claim stops two processes from both creating. `create-task {work, workRev}` makes one Task per Work item `rev`, found again by `muxa-work: w-… <rev>`, as Routine approval does. Starting a Task reuses the New Task form.
- **The same work captured twice.** `sort-capture link` joins a Capture to a Work item. `update-work merge` folds one Work item into another: it is withdrawn with `mergedInto`, and both texts stay. A Result may list `possibleDuplicates` with a reason; the client offers Link or Keep separate. Nothing links by itself.

### 4. A request runs as a hidden record of kind `assistant-run`

`request-assistant` creates a record through the same `create()` path as a Task, with kind `assistant-run`, in `<root>/assistant/<slug>-<id>/`, in a session tagged `@muxa_task`. Its `.muxa/TASK.md` holds the Capture text, purpose, scope, the result schema and the request line. Delivery, claims, the 30 s wait and recovery are ADR 0024's, unchanged.

The agent writes `.muxa/assistant/result.json` and ends its turn. The Loop's check runs in the daemon's process, as `routines/check.rs` does. It refuses a result whose request or version does not match, a finding without a source, oversize fields, a symlink or NUL. On a pass, one locked write stores the Result as a new version on the request. On a fail, the fixed fail prompt is typed; after 3 fails the request is `failed`. After publish, the daemon closes the session on its next tick (ADR 0037).

A Result has: summary (≤ 4 KiB), findings each with ≥ 1 source (URL or path, read time), open questions, next actions, file paths, `possibleDuplicates`. A newer request version marks older Results `stale`; they stay as history. A Result never changes the Work item's state.

### 5. Every place that hides `routine-run` hides `assistant-run`

One helper replaces the check for "a background run, not a Task". It covers the task list (`task_records/mod.rs`), Inbox item production (`items/produce.rs`), park (`task_records/park.rs`), the run folder (`task_records/create.rs`) and the Loop's prompt, check and final status (`loops/mod.rs`). Routine-only logic (`routines/cleanup.rs`, `check.rs`, `runs.rs`, `record.rs`) gets its own assistant version, including cleanup: a published run's folder goes when the request's next version publishes, or after 7 days. muxa for tmux's task tree (`tmux/scripts/lib.sh`) must not list `<root>/assistant/…` sessions as Tasks.

A test creates one record of each kind and checks that `task-list`, `inbox-list` and parked Tasks show the Task only.

### 6. Delivery state is derived, not invented

A request's delivery state comes from the run record and the adapter on every list: `pending` (prompt queued), `received` (send confirmed), `processing` (adapter reports busy, with its age), `unconfirmed` (a send was interrupted; never retyped automatically), `resulted`, `failed`, `timed-out` (no result after `run_timeout`; the agent is not stopped), `cancel-requested`, `cancelled`. The stored value is a cache with its time.

### 7. Agents

A request needs automatic delivery and a finished turn, as a Routine does. The delegate is `[workspace.assistant].worker`, else the workspace's default agent. A Codex agent (ADR 0028) and a Worker whose command has no adapter are refused before any run folder exists, with a message that names the reason. ADR 0028 allows Codex with `[codex] owned_threads = true`.

### 8. Budgets

Before `create()`: open `assistant-run` records per workspace ≤ `max_open_runs`; runs created today ≤ both `max_runs_per_day`. A refusal is `conflict` with the limit and when it resets. Without a daemon, `request-assistant` is `unsupported` and the clients say automatic processing is off. Captures and lists work in the bridge.

### 9. Wire: additive, `work:9`, protocol v1

| Request | Reply |
|---|---|
| `capture {clientId, text, source?, workspace?, assist?}` | `capture-saved {capture, entry, work?, request?}` |
| `sort-capture {capture, rev, action, work?, title?, workspace?}`, action `work` / `link` / `note` | `capture-saved`; a stale `rev` is `conflict` |
| `update-work {work, rev, action, title?, workspace?, into?}`, action `edit` / `later` / `complete` / `withdraw` / `reopen` / `merge` | `work-saved {work, entry}`; a stale `rev` is `conflict` |
| `create-task {…, work?, workRev?}` | `task-saved`; the record keeps `work` |
| `request-assistant {work, rev, scope, workspace?, purpose?}` | `work-saved {work, entry, request}` |
| `cancel-request {request, version}` | `work-saved {work, entry, request}` |
| `get-work {work}` | `work-detail {work, entry, captures, requests}`: every Capture the item links, sorted or not; an unknown item is `bad-request` |

`inbox-list` gains `work[]`, `captures[]` (unsorted only) and `requests[]` (those of the listed Work items). The full contract is `protocol/PROTOCOL.md`, "Captures and Work items". Old clients ignore both. `update-inbox` keeps its `i-` / `e-` ids only. Fixtures go in `protocol/fixtures/assistant/`. Clients send the new requests only at `work:9`. Branches that also raise the level renumber on merge.

### 10. Clients

- **`+`** opens Capture at `work:9`: text, optional workspace, Save, Ask Assistant, "Start a Task instead…". An older host keeps New Task. The repo header's New Task, Ask and Terminal stay. Unsent captures stay on the device as "Not sent" and are resent on connect.
- **Inbox** has three sections: Decisions (today's items), Work, Unsorted. A Work item whose Task has an open item shows "waiting on decision" with no answer button of its own. Swipe right completes the Work item, left is Later (ADR 0040: moves to the bottom, never hidden).
- **Work detail** shows Capture → Request → Result → Task → Decision, the delivery state, and Open Session while the run lives. Result files are paths; the Reader needs a pane and cannot open them yet.
- **muxa for tmux:** `prefix + N` asks for the text first (`s` save, `a` ask, `t` Task form). The Inbox shows Work rows (`l` Later, `d` complete, `a` ask). CLI: `capture`, `work`, `assist`.

### 11. Logs

New error types carry ids and paths only. Logs hold ids, sizes and states, never Capture text, briefs or Results. Capture text travels only in files (TASK.md, the store) and the one fixed prompt.

## Not decided

- Does the badge count Unsorted Captures? This ADR: no, decisions only.
- Does Unsorted show above or below Work?
- Do a Result's open questions become Inbox items? This ADR: no in PA1; they are answered by a new request version.
- How the Reader opens Result files after the run's session closes.
- Where muxa for tmux shows a live assistant run: in the Routines section, a new section, or hidden.
- The 7-day folder cleanup in §5 is an initial value.

## Consequences

- An idea is saved without a Project, deadline or agent. Asking for help is one more tap and bounded by budgets.
- Requests and Routines share one record type, delivery path and Loop engine.
- PA1 needs the daemon and an adapter with automatic delivery: Claude Code today.
- The task tree, Inbox and Loop must all know the new kind; the test in §5 catches a missed site.
- Alternatives not taken:
  - Unstarted work as Tasks: lists them in Tasks and starts an agent.
  - Work items as a new Inbox item kind: old clients would show unknown rows, and items stay questions (ADR 0033).
  - Requests typed into the workspace's Ask session: not durable, no claim or result contract (spec §10).
  - An MCP server in muxa for results: a new contract; PA2 or later.

## Delivery slices

| Slice | Scope | Acceptance |
|---|---|---|
| A0 | this ADR, `PROTOCOL.md`, fixtures, Rust, Swift and Kotlin types | fixtures round-trip in all three |
| A1 | the store | atomic writes; a failed change writes nothing; duplicate `clientId`; oversize text; unknown version refused |
| A2 | `capture`, `inbox-list` arrays | save without workspace; reconnect returns the same id and text; no session or layout change; a sentinel from the text is absent from logs |
| A3 | `update-work`, the hidden-kind helper | Later order; stale `rev` conflict; two concurrent edits, one wins; `start-task` twice makes one Task; each record kind test (§5) |
| A4 | advertise `work:9`; CLI; muxa for tmux | save-only never starts a session; keys |
| A5 | iOS | Capture, `+` fallback, three sections, unsent captures, demo |
| A6 | Android | the same |
| — | **PA0 done** | spec §11 PA0 |
| A7 | config, kind, brief, result check | counterexamples refused; errors carry ids only |
| A8 | `request-assistant`, `cancel-request`, budgets, timeout, cleanup, agent refusal | fake agent on a `-L` server through the daemon: restart, duplicate request, unknown send, timeout, invalid result, cancel with a late result, new version while v1 runs, no content in logs, layout unchanged |
| A9 | CLI `assist`, tmux `a` | bats |
| A10 | iOS: Ask Assistant, delivery state, Result view | UI tests on the demo |
| A11 | Android | the same |
| A12 | docs: `agents.md`, `inbox.md`, `work-model.md`, `SECURITY.md`, `plan.md` | — |
