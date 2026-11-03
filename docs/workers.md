# Workers and workspace memory: delivery plan

ADR 0022 is Proposed. This document makes its release gates and implementation order explicit; it does not advertise either capability or implement native role configuration.

## Delivery tracks and dependencies

1. **W: Workers and handoff.** Define a worker once on the host, choose it for a new Task, and hand the same Task to another worker through restart. Include the CLI and both native apps. Keep the existing workspace environment path; reject `worker.env` for this release.
2. **M: Workspace memory.** Supply complete WORKSPACE.md snapshots and ask an agent for a generation-bound close-out proposal. The user reviews a diff; an explicit apply performs a serialized, recoverable write. This works even when a Task uses the workspace default agent and W is unavailable.

W does not wait for M's completion detection, workspace locks or journal. M does not require a named worker. Neither includes workflow graphs, worker-specific credentials, a new daemon service or a new native role file format.

3. **Q: Inbox queue and manual-session navigation** (proposed ADR 0025). Make Inbox the list of things needing human action; retain Tasks for managed work/history and a separate manual-session destination. Shared Move down/Later state replaces local-only deferral only after its contract is implemented.
4. **S: Routines** (proposed ADR 0026). Add stable project references and daemon-owned Routines: a worker runs on a trigger, reads its sources through native tools and publishes briefings and proposals into Inbox. An approved proposal becomes one new Task that performs the action.

W execution work and Q design/implementation can progress in parallel. S0 project identity/configuration and M's observation/contract work can also proceed independently. Actual S2 collection needs verified execution, scheduling and Inbox publication/acceptance; it does not require M or the full Phase 4 workflow graph. M remains its own release rather than blocking the daily Inbox/Routine path. The current next coding slice is still W2 worker execution integration; Q/S are recorded follow-on milestones, not shipped functionality.

## Shared work model

[Inbox, Tasks and Sessions](work-model.md) defines the identities used by every milestone: Task is durable work, Session is the tmux execution/interaction location, Run is one agent execution, and Inbox entry is a request for human action. The manual-session screen is a filtered destination; the Session entity itself also hosts managed work. Manually started AI remains manual; human typing within a Task does not change Task association. A Task returning for review reuses its id instead of creating another Task.

## Product milestones across the vision

| Milestone | Vision phase | Deliverable and completion boundary | Current status |
|---|---|---|---|
| W0–W4 Workers | 3 | resolve a named/default worker, create/handoff safely, preserve Task/Session identity, observe each new run, validate native roles and clients | configuration/client preparation, internal launch/staging integration and default-agent recovery implemented; native execution/readiness/CI gates remain |
| Q0 Work/queue contract (replaced by ADR 0027 §4 and ADR 0026 §4) | 2–3 | distinct Task/Session/Run/Inbox ids, explicit associations and Task references; shared unknown-kind fixtures; no automatic manual-session adoption | proposed, not implemented |
| Q1 Daily Inbox and navigation | 2–3 | list-first Inbox; Tasks for running/history; separate manual-session entry with All Sessions inspection; host-owned Move down order; actor changes never reclassify work | Sessions filter in both apps; Move down on the record, in the CLI and the shared queue code; list-first app UI not implemented |
| Q2 Shared dispositions (Tasks: record fields, ADR 0027 §4; proposals: S3) | 2–3 | timed Later with reconnect/device consistency, attention change handling and idempotent proposal acceptance/dismissal on the shared queue | Tasks: Later and the attention rule on the host, in the CLI and the shared queue code; proposals: approve, dismiss and later on the host and in the CLI (S2/S3); app actions not implemented |
| S0 Routine configuration | 2 → 3 | `[[routine]]` in `workspaces.toml`; trigger/timezone/worker/instructions/output-policy validation and explicit enabling; Project stays a label | done in the host: strict parser and unit tests |
| S1 Triggers | 3 → 4 | schedule on the daemon tick and run-now; no overlap; one catch-up after wake; timezone/DST; last failure visible | done in the host: DST and catch-up unit tests, a real daemon starts a scheduled run |
| S2 Routine runs → Inbox | 3 → 4 | runs stored as records of kind `routine-run`; Task Loop engine with output validation as its check; staged publish of briefings/proposals with opaque state commit and `(routine, key)` dedup; no source-specific code | done with a fake agent (real tmux, daemon and bridge); one real source still to check; Codex refused until ADR 0028 |
| S3 Inbox list and clients | 3 → 4 | stored entries in Inbox with approve/dismiss/Later; approval → one Task with fixed content; Routine detail with runs and run-now; CLI first, then one app, then the other | contract (`work:1`) and CLI done; apps decode the types, no UI yet |
| M0–M5 Workspace memory (replaced by a Routine, ADR 0027 §6) | 3 | ordered completion evidence, full WORKSPACE.md snapshots, generation-bound close-out, reviewed diff/apply and recovery | M0 source/fixture audit exists; native completion and M1–M5 implementation remain |
| Workflow graph | 4 | dependencies between Tasks and rules for starting the next Task | future direction, outside W/M/Q/S slices |
| Autonomous assignment | 5 | automatic picking/execution within explicit approval, budget and permission limits | future direction, outside W/M/Q/S slices |

The order and parallel lanes are in `docs/plan.md` Next (ADR 0027): Workers first, then Routines with the Inbox list, then memory as a Routine. Q0–Q2 acceptance is in [ADR 0025](adr/0025-inbox-queue-and-scheduled-intake.md), simplified by ADR 0027; S0–S3 are in [ADR 0026](adr/0026-routines.md).


Current W0 results and the outstanding native/concurrency gates are in `docs/findings/workers-w0.md`. Proposed ADR 0024 specifies the restart lock, durable phases, delivery claim and explicit recovery actions; its default-agent recovery paths and local kill-boundary tests are implemented, while named-worker/native release acceptance remains pending.

## Implementation progress

| Slice | Implemented now | Remaining release gate |
|---|---|---|
| W0 | isolated command/argv/env/layout probes, OS-lock owner-death probe, native startup attempts | authenticated native role/profile selection and rematching; supported tmux CI results |
| W1 | shared Rust/Swift/Kotlin types and fixtures; simplified to a worker name or `default` (ADR 0027 section 1) | done; `workers` not advertised |
| W2 configuration | strict host worker parser (rejects the name `default`) and resolver; create and restart share a validated config/instruction snapshot, native argv and workspace-only env; each run records its worker; `task-list` lists workers; named create and Keep/default/named restart work through the wire in bridge and daemon | native startup with real Claude/Codex logins |
| W2 restart recovery | durable staged TASK/HANDOFF pair, process-start identity, prepared/dispatching/replaced intent, delivery claim, explicit recovery endpoint and atomic action receipts; Task/loop/close claims; legacy records without runs preserved | named-worker end-to-end execution, fresh native role/profile validation and remaining R01–R16 release cases |
| W3 clients | CLI `--worker NAME` (`default` for the workspace default); iOS/Android pickers send a name or `default`, behind the capability; recovery inspection and actions | end-to-end check against a host with real logins |
| M0 | adapter/source audit, fixture counterexamples and M01–M17 acceptance matrix in `docs/findings/memory-m0.md` | ordered completed-turn evidence, native completion probes, workspace revision/journal implementation |

The bridge now serves named create and restart through the wire (ADR 0027 section 1); `workers` stays unadvertised. Workspace memory remains disabled. Create/restart preparation resolves selection before record/file/process changes and uses direct native argv rather than a reconstructed shell command. Restart preparation checks the displayed revision, stages the selected instruction snapshot without publishing it, and preserves agent notes; normal loop/header refresh retains that run’s instruction snapshot. Resolver/staging tests cover invalid instructions and immutable snapshots. An internal real-tmux test with a fake agent also exercises same-name command/instruction edits, stale revision rejection and explicit default clearing while preserving Task/session/pane/layout and notes. This does not verify authenticated native workers. Active Chat now discards the old transcript source on pane PID/session replacement, and session-state notifications include PID and report-time changes. Retained tags, child-agent changes within an unchanged pane PID and native readiness still require separate fresh-evidence verification. Restart recovery is now implemented independently under `task-restart-recovery`: normal restart stages files before dispatch, records successful replacement identity before publishing, and finalizes run/pending state atomically. Delivery claims are durable before startup/restart/loop input; uncertain delivery never retries automatically. Recovery cancel/finish/leave actions never respawn or type a prompt, stop automatic work, and commit a bounded idempotency receipt with the resolution. Retained stages after an uncertain/cancelled operation may remain private for inspection; no unbounded terminal/document content enters records or logs.

Integrated local verification passed after the shared quoting fix and claim integration: `make check` (ShellCheck 0.11.0), `make test-tmux`, `make repro`, `make test-apple`, `make test-android`, `make build-app`, `make build-android` and `make lint-android`. The linked-close integration test covers tagged and directory-matched Tasks. These are local regression/component results; they do not close the native, supported-version CI or full recovery gates. Optional external tmux plugin checks were skipped without their environment variables.

The current W2 increment additionally tests unknown/malformed operation blocking and tampered/symlinked stage rejection through both the bridge and daemon. Kill-boundary tests also exercise competing status, loop and recovery writers while the restart owner holds the claim. The next increment adds internal real-tmux/fake-agent cases for simultaneous named restarts (one replacement), stale losers, source replacement before dispatch, invalid instruction files and uncertain tmux errors. Prepared and finalized phases now also have real bridge-kill tests, alongside the existing dispatch/delivery boundaries. The post-replaced publication/rename boundaries still lack deterministic real-kill coverage; partial-pair recovery remains fixture evidence. These extend the acceptance matrix without completing the named-worker/native release gates. This increment passed `make check`, `make test-tmux`, `make repro` and Clippy with the tmux-integration features. Checks used the installed ShellCheck 0.11.0/Bats 1.14.0 and a process-local `commit.gpgsign=false` override for temporary Git fixtures; no personal tool or Git settings changed. The final internal same-name restart/default-clear test also passed separately.

New restarted runs now keep a private observation baseline. Session listing, state notifications, Loop delivery and managed Chat use replacement PID/start identity and process-bound adapter sessions rather than retained tags. The sender and follow-up restart revalidate the current run; old sessions/transcripts never fill a new run’s history. Unknown or damaged metadata and missing managed records stay unknown. Claude’s PID session file can supply process ownership; Codex’s current cwd/time heuristic cannot. Guarded Codex runs and hook-only custom agents therefore do not receive automatic continuation and fall back to the fixed prompt in Terminal after the existing wait. Legacy waits and manual sessions keep their behavior. A further restart of an unconfirmed replacement requires explicit force; both mobile clients offer the corresponding Stop and restart confirmation for a restarted run with unknown state. This does not certify a requested native role/profile. Integrated verification of this follow-on increment passed `make check`, `make test-tmux`, `make repro`, Clippy with tmux-integration features, `make test-apple`, `make test-android`, `make build-app`, `make build-android` and `make lint-android`. The Rust tmux run includes 55 Task-record acceptance cases and 8 Chat integration cases; native role/profile and supported-version CI results are still outstanding.

Proposed ADR 0028 would replace this Codex fallback with a thread muxa creates and sends turns to; it waits for the developer's checks with a real login.

## W1 contract status

W1 defines WorkerInfo, the worker selection, the Task worker, the run worker and the optional restart revision in Rust, Swift and Kotlin. `protocol/fixtures/workers/` pins the wire forms; PROTOCOL.md describes them. ADR 0027 section 1 replaced the omitted/null/named tri-state and the run `assignment` object with a worker name. Runs written before that still read: the bridge keeps `assignment` on rewrite and lists it as a name or `default`.

The bridge and demo bridges do not advertise `workers`. Requests without `worker` keep their encoding and launcher. Restart requires the Task's workspace to stay configured; a removed workspace is rejected before staging or respawn.

## Current evidence and W0 gates

| Subject | Evidence now | What remains before implementation |
|---|---|---|
| Named Claude role | Official docs describe main-session `--agent`; installed Claude Code 2.1.316 help confirms the flag | Start a controlled native role from actual Task launch directories; observe adapter identity and readiness without sending a task prompt |
| Codex profile | Official docs and installed Codex CLI 0.166.0 help confirm file-based `--profile` | Start a controlled profile in repo/non-repo Task directories; verify config selection, adapter identity and readiness |
| Role discovery | Docs establish native roles; no muxa-specific runtime proof | Check user role, project role in a worktree, and non-repo Task under a workspace root. Do not assume discovery crosses a repo boundary. Repeat for an existing custom workspace config home |
| Respawn and environment | New worker-respawn repro passes locally on tmux 3.7c: explicit and repeated commands, argv/env, pane/layout and real desktop view | Supported tmux CI matrix and native integration; pure worker resolution is implemented |
| Restart transaction | Existing restart prepares facts and respawns; it is not proof of atomicity across writers | Implement and verify ADR 0024’s exclusive lifecycle claim, displayed-revision conflicts and explicit recovery before using the path for worker assignment |
| Agent observation | Fixture registry rematches PID snapshots; tmux retains stale hook tags across respawn | Active Chat replacement is covered by fixture and real-bridge tests; native Claude → Codex → Claude and new-run freshness remain gated |

The initial documentation/help check ran only `--help` and `--version`. Subsequent startup-only native attempts are recorded in `docs/findings/workers-w0.md`; they did not establish authenticated selection or readiness and sent no model turn. `claude agents` manages background sessions in the installed version and must not be used as a custom-role discovery probe.

W0 has two separate checks:

- **Automated, no model:** fake programs and fixture adapters on isolated `tmux -L muxa-test-*` servers. Use synthetic environment values, not real credentials. Assert bytes/argv, distinct process replacement, retained workspace environment, pane id/size/layout and desktop selection. Reuse established facts instead of re-deriving them. New tmux behavior needs a repro and a findings row.
- **Developer-run native probe:** use controlled native role/profile files with no startup task prompt or native `initialPrompt`. Do not modify or copy real role/config files. Record version, launch directory class, selected native role, whether the adapter matches, and initial state; never copy transcript or instruction content into logs. Stop before a task/model turn. If trust, login or native startup behavior cannot be checked without a turn, leave that result unverified and document the necessary manual check. Do not weaken the native gate to make a capability appear available.

W0 is complete when both supported native launch paths are verified, the explicit-command repro passes on supported tmux versions, and the restart failure/claim table below has executable acceptance cases. A failed native combination must either block W or be explicitly removed from the accepted scope; it must not silently launch another role.

## Worker configuration and resolution

Use the ADR's host-owned `[[worker]]` table: name, description, agent, args, instruction-file path, allowed workspaces and optional check defaults. Native role/tool/model/permission definitions stay native. A worker's AI is configuration; the run's observed AI/state still comes from an adapter.

Resolution is one operation used by both create and restart:

1. Read the workspace and worker configuration once for this request.
2. Resolve Keep/default/named against the current record; check the displayed revision and allowed workspace.
3. Read and validate instructions before any process or managed file change. Resolve the current command, including same-name edited workers and clearing to the current workspace default.
4. Prepare the managed header/handoff as temporary content; preserve Task notes below the managed marker.
5. Claim the Task lifecycle operation; recheck source run/pane identity and revision. Spawn only with the resolved command and existing workspace environment. Publish prepared files and finalize the run before sending its fixed prompt.

A worker name is a stable identity in W. Renaming requires explicit reassignment; a removed/disallowed name is an error, not a fallback. Workspace default is not an implicit named worker. Native args are trusted host configuration and may affect account/tool behavior; muxa adds no worker environment layer and does not promise account isolation between native roles. Existing removal of workspace env keys remains subject to ADR 0016's inherited-session limitation.

A config/instruction change during an operation applies to the next operation; do not reread half the configuration after claiming it. Loop defaults prefill creation only. Selecting a worker never creates a loop or replaces a Task's existing check/limits; an existing loop follows ADR 0019's unchanged restart semantics. Validate defaults against every permitted workspace, and require `task-loop` before offering a running loop.

W1 validation passed: `make check` (ShellCheck 0.11.0), `make test-apple`, `make test-android`, `make build-app`, and the bridge Task-record integration suite (35 tests). W0’s isolated tmux probes and repro checks also passed locally.

## Selection contract (ADR 0027 section 1)

| Input/state | Meaning |
|---|---|
| create-task omits worker, or sends `default` | workspace default |
| create-task sends `Reviewer` | Reviewer |
| restart-agent omits worker (legacy client) | keep the current worker |
| restart-agent sends `default` | clear the worker; workspace default |
| restart-agent sends `Reviewer` | select Reviewer |
| Task record omits worker | workspace default |
| run has worker `default` or `Reviewer` | the worker requested for that run |
| legacy run omits worker | unrecorded, not the workspace default |

A worker-aware client always sends a name. `rev` is optional: when sent, a stale revision conflicts before files, loop state or process changes. A host worker may not be named `default`. A removed or disallowed worker is `bad-request`, also on Keep; there is no fallback. JSON `null` reads as omitted.

## Restart failure and concurrency table

| Point | Required behavior |
|---|---|
| Invalid worker reference, scope, args shape, instruction file or revision | fail before stopping the existing agent or publishing new headers |
| Another lifecycle operation owns the Task | conflict/busy with no second respawn; the daemon and fallback bridges honor the same claim |
| Native role/profile reference is missing or invalid | native validation may fail after replacement; retain the requested assignment and expose native startup failure in Terminal, never claim the role was confirmed or silently choose another worker |
| Native process still running | existing force confirmation applies; selected worker does not imply permission to stop it |
| Claim acquired, before respawn | persist operation intent, expected revision, source run/pane and intended assignment; stage files without replacing live TASK.md |
| Respawn definitely refused | fail safely; previous run/assignment and managed files stay authoritative |
| Respawn succeeded | publish prepared files, finalize explicit run assignment, invalidate old adapter binding, then schedule the fixed prompt for the new run |
| Crash or error with uncertain process outcome | retain uncertain intent, show requested/unconfirmed selection, send no automatic retry prompt or replacement; refresh and require an explicit recovery action |
| Two worker-aware clients restart the same displayed rev | one operation can win; the other conflicts, including after the winning claim is released |
| Same worker name, native role/command edited | launch the new resolved command; never combine new instructions with the old start command |
| Old pane state tag survives process replacement | show no confirmed new agent identity from that tag alone; bind fresh adapter evidence to the new run |

Lifecycle operations need serialization in addition to per-record writes. Document the claim/record-lock order before implementation; do not assume separate `Store::write` calls make an external tmux side effect atomic. A claim failure must not partially rewrite the Task brief. Use controllable barriers/fault injection around respawn and record/file publication; do not test crashes only after a successful request has ended.

## Release W implementation sequence

| Phase | Scope/files | Acceptance and handoff |
|---|---|---|
| W0 Facts and failure model | `scripts/repro/`, findings, adapter fixtures, restart design | no-model A → B → default/same-name probes, native launch observations, concurrency/crash table; ADR W scope accepted before production changes |
| W1 Contract | PROTOCOL.md, fixtures, Rust protocol/record types, Swift/Kotlin protocol types | name/`default`/omitted round trips, recorded and legacy run workers, optional revision, old record unknown-field preservation; no capability advertised yet |
| W2 Host and CLI | workspace/worker parsing, create/restart resolver and lifecycle claim, header/handoff, CLI flags and JSON | create named/default; Keep/set/clear with current command; disallowed/removed names; A → B → A → default; argument bytes; preflight failures; simultaneous daemon/fallback requests; uncertain-side-effect recovery; no content/secrets in logs; unchanged pane/layout |
| W3 Apps and demo | worker pickers in New Task/restart, handoff confirmation, detail/run/Inbox labels, demo fixtures; maintain the shared Task/Run/Session model | both apps: workspace filtering, capabilities absent, removed current worker, explicit default vs Keep, stale-revision conflict, rejected restart preserves current UI, unconfirmed operation presentation |
| W4 Release verification | example config, docs/agents.md, README, plan, SECURITY.md | make check, make test-tmux, make repro, make test-apple, make test-android, app/UI tests and Android lint; native checks recorded; advertise workers only for complete host support |

The existing tmux-agents popup already carries full Task records through CLI JSON. Named worker creation can use the built-in launcher until tmux-agents' command/environment contract is verified. No new user-session metadata options are part of W, and tmux sidebar/status cache work remains ADR 0023 slice 2.

## Release M implementation sequence

| Phase | Scope | Acceptance |
|---|---|---|
| M0 Observations and transaction model | fresh turn/run identity, delivery claims, canonical-path memory lock, apply journal/revision and recovery design | initial old done/idle is eligible for delivery but cannot complete it; exclusive delivery across writers; lock order and journal recovery have precise transitions; do not ship on file-existence-only readiness |
| M1 Contract | independent workspace-memory capability, generation/run/revision identity, requests/states/errors, Reader kinds, CLI tokens and cross-language fixtures | no named worker required; unknown states have no active apply controls; missing vs empty distinguished; stale generation/revision conflicts; no capability until implementation complete |
| M2 Snapshots and proposals | full snapshots, generation-specific instructions/output, immutable completed proposal in bridge state, bounded Reader/diff | snapshot bytes/existence/hash/revision captured under one workspace lock after pending-journal recovery; crash-after-rename → create/restart/close-out and revision-bearing proposal reads recover or refuse; 64 KiB+1 rejected without truncation; multibyte boundary; missing source removes stale snapshot; old proposal/run cannot become ready; changed live base cannot silently rebase |
| M3 Apply/discard and recovery | Task → workspace lock, compare/backup/rename, prepared/committed journal, operation identity and idempotency | independent daemon/fallback writers, restart/discard vs apply, aliases of the same canonical path, crash before/after rename, identical bytes from different generations, ABA updates, external edit conflicts, symlink parents/swap races, process-death release, no content in logs |
| M4 Apps and demo | Close out before Done, optional skip, readiness/failure/retry, immutable diff review, Apply/Discard | explicit action and tokens, conflict refresh never auto-applies, cancellation/failed apply never marks Done, reconnect restores generation, no-session fallback, independent capability combinations |
| M5 Release verification | trust boundary, examples, Reader docs, plan and manual native turn check | all shared protocol/host/app checks; completion proven by adapters in real native runs before advertising workspace-memory |

Snapshot locking applies at creation, restart and close-out, not just apply. Snapshots and revision-bearing proposal reads first recover a prepared journal operation, or return busy/conflict for a separate ordered recovery pass; renamed bytes must never be paired with an old revision. If both Task and workspace locks are needed, order is Task → workspace. Different workspace names resolving to the same canonical memory path share the lock and revision/journal identity. Journal recovery cannot acquire another Task lock under a workspace lock; a separate ordered recovery pass must be specified and tested.

## What is ready next

ADR 0027 section 1 and named create/restart are done through the wire, in the bridge and the daemon, with the CLI and both app pickers behind the capability. Codex continues restarted runs by hand (fixed prompt in Terminal).

Before `workers` is advertised, the developer checks it natively: create a Task with a named Claude worker and a named Codex worker with real logins, then restart A → B → `default` → A. Record the versions and results in `docs/findings/workers-w0.md`. The supported tmux CI matrix is a separate Phase 1 lane. ADR 0027 section 3 lists the remaining R01–R16 cases as known gaps, not gates.

## Restart-recovery integration evidence

The recovery protocol fixtures, CLI tests and Swift/Kotlin codec/session tests pass. Both app builds, Android JVM tests and Android lint pass with recovery UI. The bridge/daemon integration cases exercise prepared cancel, stale revision, action mismatch, partial-pair finish, external edit conflict, exact receipt retry and conflict after a newer revision. A legacy delivery with no recorded run resolves without inventing history.

Harness-only control-mode proxies hold dispatch or its reply while a real bridge is killed. Separate cases cover before replacement, after native process replacement but before its reply, before input, and after partial input. The durable state remains blocked, a second restart conflicts, reconnection does not retry, and explicit resolution leaves the observed process/files/input unchanged. This is local default-agent execution evidence, not proof of named native roles or all R01–R16 combinations. Existing loop regressions also cover legacy Tasks without run history.

Final recovery verification passed: `make check`, the full `make test-tmux` suite (run separately from heavy builds), `make repro`, `make test-apple`, `make test-android`, `make build-app`, `make build-android` and `make lint-android`. The final default-agent integration includes legacy no-run records and atomic pending/run/loop bookkeeping. Optional external-plugin checks remain skipped unless configured; native role/profile and supported-version CI gates are not claimed complete.
