# Workers and workspace memory: delivery plan

ADR 0022 is Proposed. This document makes its release gates and implementation order explicit; it does not advertise either capability or implement native role configuration.

## Delivery tracks and dependencies

1. **W: Workers and handoff.** Define a worker once on the host, choose it for a new Task, and hand the same Task to another worker through restart. Include the CLI and both native apps. Keep the existing workspace environment path; reject `worker.env` for this release.
2. **M: Workspace memory.** Supply complete WORKSPACE.md snapshots and ask an agent for a generation-bound close-out proposal. The user reviews a diff; an explicit apply performs a serialized, recoverable write. This works even when a Task uses the workspace default agent and W is unavailable.

W does not wait for M's completion detection, workspace locks or journal. M does not require a named worker. Neither includes workflow graphs, worker-specific credentials, a new daemon service or a new native role file format.

3. **Q: Inbox queue and manual-session navigation** (proposed ADR 0025). Make Inbox the list of things needing human action; retain Tasks for managed work/history and a separate manual-session destination. Shared Move down/Later state replaces local-only deferral only after its contract is implemented.
4. **S: Scheduled project intake** (proposed ADR 0025). Add stable project references and daemon-owned Routines, such as an 08:00 Slack collection that publishes a briefing and bounded proposals into Inbox. It does not automatically execute the proposals.

W execution work and Q design/implementation can progress in parallel. S0 project identity/configuration and M's observation/contract work can also proceed independently. Actual S2 collection needs verified execution, scheduling and Inbox publication/acceptance; it does not require M or the full Phase 4 workflow graph. M remains its own release rather than blocking the daily Inbox/Routine path. The current next coding slice is still W2 worker execution integration; Q/S are recorded follow-on milestones, not shipped functionality.

## Shared work model

[Inbox, Tasks and Sessions](work-model.md) defines the identities used by every milestone: Task is durable work, Session is the tmux execution/interaction location, Run is one agent execution, and Inbox entry is a request for human action. The manual-session screen is a filtered destination; the Session entity itself also hosts managed work. Manually started AI remains manual; human typing within a Task does not change Task association. A Task returning for review reuses its id instead of creating another Task.

## Product milestones across the vision

| Milestone | Vision phase | Deliverable and completion boundary | Current status |
|---|---|---|---|
| W0–W4 Workers | 3 | resolve a named/default worker, create/handoff safely, preserve Task/Session identity, observe each new run, validate native roles and clients | configuration/client preparation and default-agent recovery implemented; worker execution/native/CI gates remain |
| Q0 Work/queue contract | 2–3 | distinct Task/Session/Run/Inbox ids, explicit associations and Task references; shared unknown-kind fixtures; no automatic manual-session adoption | proposed, not implemented |
| Q1 Daily Inbox and navigation | 2–3 | list-first Inbox; Tasks for running/history; separate manual-session entry with All Sessions inspection; host-owned Move down order; actor changes never reclassify work | proposed, not implemented; existing slice 1 remains |
| Q2 Shared dispositions | 2–3 | timed Later with reconnect/device consistency, attention change handling and idempotent proposal acceptance/dismissal on the shared queue | proposed, not implemented |
| S0 Project/Routine foundation | 2 → 3 | stable project id, rename-safe Routine references, timezone/source/collector/output limits and explicit enabling | proposed; Project currently remains free text |
| S1 Durable schedule | 3 → 4 | daemon runs while clients disconnect; no overlap; bounded wake catch-up; timezone/DST, failure history and uncertain-run recovery | proposed, not implemented |
| S2 Source → Inbox | 3 → 4 | one read-only Slack digest path with provenance, cursor/batch coherence, deduplication and bounded proposals; explicit acceptance before downstream work | proposed, not implemented; source auth and native collector verification required |
| S3 Routine UX/release | 3 → 4 | configure/pause/run-now/history, project-scoped Inbox flow, real-source/device/crash tests and capability gates | proposed, not implemented |
| M0–M5 Workspace memory | 3 | ordered completion evidence, full WORKSPACE.md snapshots, generation-bound close-out, reviewed diff/apply and recovery | M0 source/fixture audit exists; native completion and M1–M5 implementation remain |
| Workflow graph | 4 | dependencies between Tasks and rules for starting the next Task | future direction, outside W/M/Q/S slices |
| Autonomous assignment | 5 | automatic picking/execution within explicit approval, budget and permission limits | future direction, outside W/M/Q/S slices |

The preferred delivery path is reliable W execution → Q daily Inbox → S scheduled intake, with M progressing in parallel. Q0/Q1 and S0 design need not wait for Workers release verification. The detailed Q0–Q2/S0–S3 acceptance contract remains in [ADR 0025](adr/0025-inbox-queue-and-scheduled-intake.md).


Current W0 results and the outstanding native/concurrency gates are in `docs/findings/workers-w0.md`. Proposed ADR 0024 specifies the restart lock, durable phases, delivery claim and explicit recovery actions; its default-agent recovery paths and local kill-boundary tests are implemented, while named-worker/native release acceptance remains pending.

## Implementation progress

| Slice | Implemented now | Remaining release gate |
|---|---|---|
| W0 | isolated command/argv/env/layout probes, OS-lock owner-death probe, native startup attempts | authenticated native role/profile selection and rematching; supported tmux CI results |
| W1 | shared Rust/Swift/Kotlin types and fixtures | contract-only implementation complete; no capability enabled |
| W2 configuration | strict host worker parser and pure resolver; scope, fresh bounded instructions, workspace-only env, native argv and loop defaults | connect resolved selection to create/restart only with the complete execution/recovery path |
| W2 restart recovery | durable staged TASK/HANDOFF pair, process-start identity, prepared/dispatching/replaced intent, delivery claim, explicit recovery endpoint and atomic action receipts; Task/loop/close claims; legacy records without runs preserved | named-worker execution integration, fresh native binding and remaining R01–R16 release cases |
| W3 clients | worker selection remains capability-gated; CLI/iOS/Android now also inspect and explicitly resolve restart recovery with displayed revision/id, phase/action intersection and unconfirmed history | named-worker host integration and end-to-end native release verification |
| M0 | adapter/source audit, fixture counterexamples and M01–M17 acceptance matrix in `docs/findings/memory-m0.md` | ordered completed-turn evidence, native completion probes, workspace revision/journal implementation |

Named worker execution and workspace memory remain disabled. Restart recovery is now implemented independently under `task-restart-recovery`: normal restart stages files before dispatch, records successful replacement identity before publishing, and finalizes run/pending state atomically. Delivery claims are durable before startup/restart/loop input; uncertain delivery never retries automatically. Recovery cancel/finish/leave actions never respawn or type a prompt, stop automatic work, and commit a bounded idempotency receipt with the resolution. Retained stages after an uncertain/cancelled operation may remain private for inspection; no unbounded terminal/document content enters records or logs.

Integrated local verification passed after the shared quoting fix and claim integration: `make check` (ShellCheck 0.11.0), `make test-tmux`, `make repro`, `make test-apple`, `make test-android`, `make build-app`, `make build-android` and `make lint-android`. The linked-close integration test covers tagged and directory-matched Tasks. These are local regression/component results; they do not close the native, supported-version CI or full recovery gates. Optional external tmux plugin checks were skipped without their environment variables.

## W1 contract status

W1 now defines WorkerInfo, presence-preserving Keep/default/named selection, Task worker, explicit run assignment and optional restart revision in Rust, Swift and Kotlin. `protocol/fixtures/workers/` pins the shared wire forms; PROTOCOL.md describes the extension within v1. Known/unknown assignment tags remain distinguishable from legacy missing assignment. Older record/run extras, including unknown assignment subfields, survive rewrite.

This is contract preparation authorized by the developer while W0 remains incomplete. The bridge and demo bridges do not advertise `workers`. Named create, explicit restart selection/clear, and restart revision are unsupported before side effects. A stored named worker also blocks a legacy Keep restart on this W1 host. Existing default Task requests keep their encoding and behavior. Host config parsing, CLI selection flags and app pickers are now prepared in W2/W3 as shown above. Worker execution still requires its integration gates. The independent restart-recovery implementation is described above.

## Current evidence and W0 gates

| Subject | Evidence now | What remains before implementation |
|---|---|---|
| Named Claude role | Official docs describe main-session `--agent`; installed Claude Code 2.1.316 help confirms the flag | Start a controlled native role from actual Task launch directories; observe adapter identity and readiness without sending a task prompt |
| Codex profile | Official docs and installed Codex CLI 0.166.0 help confirm file-based `--profile` | Start a controlled profile in repo/non-repo Task directories; verify config selection, adapter identity and readiness |
| Role discovery | Docs establish native roles; no muxa-specific runtime proof | Check user role, project role in a worktree, and non-repo Task under a workspace root. Do not assume discovery crosses a repo boundary. Repeat for an existing custom workspace config home |
| Respawn and environment | New worker-respawn repro passes locally on tmux 3.7c: explicit and repeated commands, argv/env, pane/layout and real desktop view | Supported tmux CI matrix and native integration; pure worker resolution is implemented |
| Restart transaction | Existing restart prepares facts and respawns; it is not proof of atomicity across writers | Implement and verify ADR 0024’s exclusive lifecycle claim, displayed-revision conflicts and explicit recovery before using the path for worker assignment |
| Agent observation | Fixture registry rematches PID snapshots; tmux retains stale hook tags across respawn | Native Claude → Codex → Claude, active-chat rebinding and new-run freshness remain gated |

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

## Assignment contract defined in W1

These wire shapes are now defined in PROTOCOL.md; their execution semantics remain gated on W2.

| Input/state | Meaning |
|---|---|
| create-task omits worker or sends null | use workspace default |
| restart-agent omits worker | Keep current assignment |
| restart-agent has worker: null | clear assignment; workspace default |
| restart-agent has worker: "Reviewer" | select Reviewer |
| Task record omits worker | workspace default assignment |
| new run assignment: {kind: workspace-default} | default was explicitly selected for that run |
| new run assignment: {kind: worker, name: Reviewer} | named worker selected for that run |
| legacy run omits assignment | historical assignment unrecorded |

`rev` on restart is additive. Worker-aware clients send the displayed record revision even for Keep current. A stale revision conflicts before files, loop state or process changes. Explicit worker selection/clearing without `rev` is bad-request; omission of both fields remains the legacy Keep path. Older clients can continue generic restarts through the same resolved-current-assignment path; they cannot choose/clear a worker without capability support.

Do not deserialize the request with a codec that merges missing and null. Model Keep, Default and Named explicitly, and pin all three serialized forms in Rust, Swift and Kotlin fixtures. Run history uses an explicit assignment enum to avoid treating legacy absence as proof of a default. Unknown future assignment kinds display as unknown; do not reinterpret them as Workspace default.

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
| W1 Contract | PROTOCOL.md, fixtures, Rust protocol/record types, Swift/Kotlin protocol types | omitted/null/named round trips, explicit/default/unknown run assignments, optional revision, old record unknown-field preservation; no capability advertised yet |
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

The next W deliverable is **connecting worker resolution to create/restart and fresh native run binding**, with remaining R01–R16 acceptance cases and W0 native/CI evidence. Recovery staging, delivery claims, API and client actions are implemented; their local acceptance evidence is recorded below. The developer explicitly authorized W1 contract/type/fixture work while W0 remains incomplete. The developer later authorized parallel W2/W3 groundwork and client preparation. This does not enable workers or waive native, recovery or end-to-end release gates. Memory's design stays reviewable separately while Workers becomes concrete. Record remaining uncertainty explicitly; do not implement per-worker env or infer new agent state to avoid a failed gate.

## Restart-recovery integration evidence

The recovery protocol fixtures, CLI tests and Swift/Kotlin codec/session tests pass. Both app builds, Android JVM tests and Android lint pass with recovery UI. The bridge/daemon integration cases exercise prepared cancel, stale revision, action mismatch, partial-pair finish, external edit conflict, exact receipt retry and conflict after a newer revision. A legacy delivery with no recorded run resolves without inventing history.

Harness-only control-mode proxies hold dispatch or its reply while a real bridge is killed. Separate cases cover before replacement, after native process replacement but before its reply, before input, and after partial input. The durable state remains blocked, a second restart conflicts, reconnection does not retry, and explicit resolution leaves the observed process/files/input unchanged. This is local default-agent execution evidence, not proof of named native roles or all R01–R16 combinations. Existing loop regressions also cover legacy Tasks without run history.

Final recovery verification passed: `make check`, the full `make test-tmux` suite (run separately from heavy builds), `make repro`, `make test-apple`, `make test-android`, `make build-app`, `make build-android` and `make lint-android`. The final default-agent integration includes legacy no-run records and atomic pending/run/loop bookkeeping. Optional external-plugin checks remain skipped unless configured; native role/profile and supported-version CI gates are not claimed complete.
