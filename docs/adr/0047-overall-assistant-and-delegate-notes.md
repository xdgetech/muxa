# 0047: Delegate notes, the overall assistant and the morning plan (Personal Assistant PA2)

## Status

Proposed on 2026-11-09. It implements stage PA2 of ADR 0045 ([specification](../personal-assistant.md) §6, §7, §9, §11) on top of ADR 0046, and gives a delegate memory across runs. It adds to ADR 0046 §2, §4, §9 and §10 without changing them. It does not implement ADR 0022 release M or ADR 0027 §6 (WORKSPACE.md); delegate notes are a different file with a different reader. Slices B0–B10 are implemented.

## Context

- A request runs as a hidden `assistant-run` with a brief built by `assistant/brief.rs`: the Work item, its Captures, the request's earlier Results and Tasks (ADR 0046 §4). Nothing else carries from one run to the next. The user wants each workspace's delegate to feel like one staff member (the spec's Workspace 担当), and a secretary (全体担当, the overall assistant) that coordinates them.
- Today the user picks the workspace of every request (ADR 0046 §1). Unsorted Captures and Work items without a workspace have nobody to sort them.
- Spec §6 fixes the trust shape: the overall assistant gets permitted summaries, never the workspaces' raw files or credentials; what a workspace shares with the overall assistant and what it may pass to another workspace are separate permissions; text from documents and tools is data.
- Spec §7 limits proposals to the morning top 3 and an update on demand. Later (ADR 0040), manual priority and the current selection stay as they are.
- The Routine scheduler has an occurrence model with one catch-up and DST handled (ADR 0026 §2, 0036 §3). Budgets and the day boundary exist in `assistant/policy.rs`.
- Spec §12 leaves open where the overall assistant lives and runs, and how results travel.

## Decision

### 1. A delegate keeps notes in its workspace

`<root>/.muxa/assistant/NOTES.md` is the delegate's memory: durable facts about the workspace's work, the user's stated preferences for it, and where things are. No Task progress, no Capture text copied whole, no secrets, no content about another workspace. The brief states these rules. It lives in `.muxa/` as `routines.toml` does (ADR 0036): anyone who can write the root can edit it, and the user keeps it in git when they want history.

- **Read:** at every run's start, the host copies the live file into the run folder as `.muxa/assistant/NOTES.md` with its SHA-256 (the base). At most 16 KiB of UTF-8 without NUL, a regular file; a larger or invalid file fails the start with the path and size, before any session exists. The brief points to the copy.
- **Propose:** `result.json` gains optional `notes`: the full replacement text, at most 16 KiB. The check applies the result's rules. The host stores it on the Result as `notes {text, base}`. Nothing is written to the live file.
- **Apply:** the user reads the diff in Work detail and taps Update notes: `apply-notes {source, version, base}`. The host checks that the live file's hash is still `base` (else `conflict`: ask again), then writes it atomically. No Task, generation, journal or backup (ADR 0027 §6). `[workspace.assistant] notes = "auto"` lets the host apply a passing proposal at publish; the default is `"review"`, because a web page or document read by the run can plant text that would then persist into every later run.
- **Forget:** edit or delete the file. The next run sees the edited file. Stored Results keep their `notes` as history, as they keep summaries.

### 2. The overall assistant is a workspace of its own

The overall assistant is configured as a `[[workspace]]` with `[workspace.assistant] overall = true`, so its root, agent command, env (ADR 0016), worker, `create()`, delivery and budgets are the ones that exist. One workspace at most may be `overall`. It is not the Personal Workspace: its root holds only its notes and run folders. Clients leave it out of the New Task and Capture workspace pickers. Its agent's native tools are the user's configuration; muxa passes it no other workspace's credentials and reads no other root for it. This is the spec's non-sandbox statement, repeated: the host filters what the brief carries; the OS does not.

### 3. What the overall assistant reads

The host builds its brief, and only the host. Per participating workspace, `[workspace.assistant] share` says what crosses:

| `share` | In the brief |
|---|---|
| `none` (no `[workspace.assistant]`) | nothing, not even the workspace's name |
| `outline` | Work item titles, states, `laterAt`, `pinnedAt`, `due`; request scopes and delivery states with their times; open Inbox items' kinds, titles and ages |
| `summaries` (default with `delegate = true`) | `outline`, plus each Work item's newest current Result: its summary cut to 600 characters, at most 3 open questions and 3 next actions of 200 characters each, and its number of findings |

Closed Work items appear only when they changed since the previous plan. The shared part of the brief is at most 32 KiB; when it is over, the oldest items are left out first and the brief says how many. Never: findings' text and sources, Capture text of a workspace, findings' file paths, Result `files`, notes of a delegate, TASK.md, pane content. Unsorted Captures and Work items without a workspace belong to nobody, so the overall assistant reads them whole: sorting them is its job. The brief ends with the overall notes (§1 applies to the overall workspace too) and the result schema.

### 4. The overall assistant proposes; the user approves

An overall run writes `.muxa/assistant/plan.json`. Besides the plan (§5), it holds `proposals[]`, each one of:

- `sort {capture, action, workspace?, title?, work?}`: as `sort-capture` would do it;
- `delegate {work, workspace, scope, purpose, reason}`: as `request-assistant` would. `purpose` is at most 1 KiB and carries the ask, not another workspace's summary. The brief says so; the host cannot check it.

The host stores each as an Inbox item of kind `sort` or `delegate` with answers `approve, reject, later`. Approve runs the stored action once, keyed by the item's id and hash as Routine approval is (ADR 0026, Implementation notes); a second approve returns the same item. Reject answers it. The overall assistant never sends a request itself unless the receiving workspace sets `[workspace.assistant] overall_requests = "auto"` (default `"review"`; `"none"` refuses even approved ones). An `auto` request still counts against every budget of ADR 0046 §8 and against `[assistant] max_auto_requests_per_day` (default 5); at most one per workspace per plan. Delegation stays one level deep: a delegate's `openQuestions` that need another workspace come back to the user as questions, not as requests.

### 5. The plan: morning top 3 and on demand

A plan is a record `plans/p-<8hex>.json` in the assistant store, made by one overall run. The run is an `assistant-run` whose record names the plan instead of a request (`RunFor` gains the variant); delivery, check, fail prompt, timeout, cleanup and the hidden-kind helper are ADR 0046 §4–§6, unchanged.

- **Trigger:** `[workspace.assistant] plans = ["30 7 * * 1-5"]` with `timezone`: at most 3 schedules, each parsed and scheduled as a Routine's `schedule` (ADR 0036 §3), one catch-up, DST once. The default is one morning plan, `"30 7 * * 1-5"` in the host's own zone when `timezone` is absent; a user who wants a midday or evening plan adds a line, and `plans = []` plans on demand only. Schedules due at once make one plan. `plan-now {minutes?}` starts one on demand; `minutes` is the time the user has. One open plan run at a time; a second is `conflict`.
- **Inputs:** §3's brief, the open Inbox items, the previous plan, and nothing about the calendar unless the overall workspace's own tools supply it.
- **Output:** `top[]`, at most 3, each `{work | item, why, next, minutes {low, high, estimated}, basis[] {kind: due | dependency | goal | time | manual, text, source?, at?}, missing[]}`; fewer than 3 is fine. `unanswered[]` and `stale[]` are not the agent's to say: the host derives them when listing, from delivery states and from Results older than their Work item's `rev`, as it derives delivery (ADR 0046 §6). The host also derives `changed[]` against the previous plan (entered, left, moved) and stamps `inputsAt` and the workspaces included.
- **Rules the host enforces:** a pinned Work item (§6) takes the first slots in pin order; the agent fills the rest. An item with `laterAt` is never listed: Later means the user moved it down. A Later item with a `due` within 24 hours shows in a one-line "Due soon, moved down by you" note under the top rows, not in them. The plan never changes a Work item, an Inbox item, their order, or `laterAt`. A plan with no candidates publishes empty and says so.
- **In the Inbox:** a Today section above Decisions shows the newest plan's rows with the reason, the minutes and a Why disclosure; a row opens its Work item or Inbox item. A new plan never changes the client's selection or scroll; the section shows a New marker. The host keeps 7 plans.

### 6. Manual priority and due

`update-work` gains `pin` and `unpin` (`pinnedAt`): pinned items are listed first in `work[]`, then as today. `edit` takes `due {at, timezone}`, set by the user only; an estimated deadline lives in a plan's `basis`, never on the item.

### 7. Wire: additive, `work:9`, protocol v1

| Request | Reply |
|---|---|
| `apply-notes {source, version, base}`, `source` a request or a plan | `work-saved {work, entry, request}` or `plan-saved {plan, entry}`; a changed live file is `conflict` |
| `plan-now {minutes?}` | `plan-saved {plan, entry}`; no overall workspace is `unsupported`, an open plan run `conflict` |
| `update-work {…, action: pin / unpin, due?}` | `work-saved` |
| `update-inbox {entry, action}` on a `sort` or `delegate` item | `inbox-saved`, with `capture-saved` or `work-saved` on approve |

`inbox-list` gains `plans[]` (newest first) and `assistant {overall?, delegates[] {workspace, share, overallRequests, notesAt?}}`. A Result gains `notes?`; a Work item `pinnedAt?` and `due?`; the hidden-kind test of ADR 0046 §5 gains a plan run. The full contract is `protocol/PROTOCOL.md`, "Plans and the overall assistant". Fixtures in `protocol/fixtures/assistant/`. Clients send the new requests only at `work:9`; a level 8 host answers them `bad-request`, and old clients ignore the new arrays and kinds.

### 8. Clients

- **Work detail:** the proposed notes text with Update notes (clients keep no copy of the live file, so no diff); the plan's reason when the item is in Today.
- **Inbox:** Today section; `sort` and `delegate` items with Approve, Reject, Later; the delegate's name is the workspace's.
- **muxa for tmux:** Today rows in `prefix + T` with `o` to open the target; `t` plan now. CLI: `plan`, `plan now`, `assist notes WORK --apply`.

### 9. Logs and budgets

Logs carry ids, counts, hashes, states and schedule times; never notes, plan texts, reasons, purposes or summaries. Overall runs count against `[assistant] max_runs_per_day` and the overall workspace's own limits (defaults: `max_open_runs` 1, `max_runs_per_day` 8, `run_timeout` 15m). A reached limit stops automatic plans with a visible reason and a resume, as spec §8 asks: the occurrence is kept as a `failed` plan with its `reason`, and the next occurrence runs again.

## Not decided

- Whether the check scans `notes` and `purpose` for secret shapes (key prefixes, PEM headers). This ADR: size and encoding only; the brief carries the rule.
- Whether an unsorted Capture whose text names a workspace may be sorted by `auto`. This ADR: sorting is always reviewed.
- Whether a plan run also reads the calendar through the overall workspace's tools. (The default time is weekdays at 07:30, §5.)
- Whether the `minutes` of `plan-now` persists as the user's stated free time for the day.
- A hosted (cloud) overall assistant reading filtered summaries over MCP was considered and deferred: work data would leave the host, and its output would need the same checks.
- PA3 (policies, hypotheses, correction history) is out of scope. PA2 leaves room: a plan's `basis` has a `kind`, `changed[]` records what moved, and `plans[]` is history; a later ADR adds the user's corrections to it.

## Consequences

- A delegate reads its own notes and proposes updates; the user sees the diff. Continuity without a transaction model, at the cost of no ABA protection on a small Markdown file, as ADR 0027 §6 accepted.
- The overall assistant is a workspace, so no new root, launcher, env or budget code. What it sees is a host filter, one function to test with synthetic data (spec §11: no raw text or credential of a sharing workspace reaches its brief or logs).
- Captures finally have a sorter, and the user still approves every sort.
- Plans, requests and Routines share one record kind, one Loop engine and one scheduler.
- Alternatives not taken:
  - Notes inside WORKSPACE.md (ADR 0022 M): one file for two readers (every Task's agent and the delegate) and ADR 0027 already made it a Routine.
  - A long-lived delegate session per workspace: it needs a restart path and holds context nobody reviewed; a run plus notes is inspectable.
  - The overall assistant as host code without an agent: ranking needs judgment the host cannot write as rules.
  - A Routine as the morning trigger: a Routine publishes briefings and proposals; a plan is a different record with host-derived marks.
  - Auto-delegation by default: spec §6 wants explicit scope per workspace.

## Implementation notes (B5)

- **Where items live.** A plan's proposal items are kept in its run's item file (`items/<run>.json`), keyed `proposal:<plan>:<index>`, so a second scan makes none. They have no `task`. Only the newest plan's proposals are asked; one whose Capture, or Work item and workspace, an open item already asks about makes no second item.
- **Approve once.** The item file keeps each approval by item id and proposal hash: the `rev` it ran at, its claim while it runs, and the ids it made. A retry after a crash sends the `rev` the request was asked at, so `request-assistant` finds that version instead of making one; a sort's single write is found by the Capture's `rev` plus one.
- **`auto` keeps the item.** The host approves the item itself and leaves it `answered` with a summary that says so, instead of sending without an item: the user sees every request the overall assistant sent, and a second approve returns it. Past a budget the item stays open with the limit in its summary, and the host does not try it again.
- **A sorted Capture.** Approving a `sort` whose Capture was sorted meanwhile resolves the item with the reason; the reply is `conflict`.
- **Sort fields.** The plan file takes a sort's `workspace` and `title` with action `work` only, as `sort-capture` does, so an approved proposal always runs.

## Delivery slices

| Slice | Scope | Acceptance |
|---|---|---|
| B0 | this ADR, `PROTOCOL.md`, fixtures, Rust, Swift and Kotlin types | fixtures round-trip in all three |
| B1 | notes: snapshot, `notes` in the result, `apply-notes`, `notes = "auto"` | oversize or symlinked file fails before a session; a changed base is `conflict`; `auto` writes at publish; a sentinel from the notes is absent from logs |
| B2 | `overall`, `share`, the brief filter | synthetic workspaces at each `share` level: the brief holds exactly the allowed fields; a sentinel in Capture text, paths and a delegate's notes never appears; `none` leaves no name |
| B3 | plan record, `plan.json` check, `RunFor` plan variant, hidden-kind test | counterexamples refused (4 items, a pinned item not first, a `laterAt` item in `top`); derived `unanswered`, `stale`, `changed` |
| B4 | plan schedules, `plan-now`, budgets | fake agent on a `-L` server through the daemon: one occurrence, one catch-up, DST, an open run conflicts, the limit stops the morning run with a reason |
| B5 | `sort` and `delegate` items, approve once, `overall_requests`, `max_auto_requests_per_day` | approve twice makes one request; `auto` respects every budget and one per workspace; `none` refuses |
| B6 | `pin`, `unpin`, `due` | list order; a stale `rev` conflicts |
| B7 | advertise `work:9`; CLI; muxa for tmux | bats; `plan now` without an overall workspace is `unsupported` |
| B8 | iOS: proposed notes, Today, new items | UI tests on the demo |
| B9 | Android | the same |
| B10 | docs: `agents.md`, `inbox.md`, `work-model.md`, `SECURITY.md`, `plan.md` | — |
| — | **PA2 done** | spec §11 PA2: results merged without aggregated credentials; unanswered and stale shown; Later, pins and the selection untouched |
