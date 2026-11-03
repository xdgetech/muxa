# Workspace memory M0: observations and acceptance boundary

ADR 0022 remains Proposed. This audit prepares M0/M1 independently of Workers: workspace-default Tasks must support memory without the `workers` capability. No snapshot, close-out endpoint, apply journal, Reader workspace document, or `workspace-memory` capability is implemented by this report. W0/W2 lock evidence does not certify a memory transaction.

## Current source and fixture evidence

| Component | What existing code establishes | Gap for memory |
|---|---|---|
| `agents/claude.rs`, `SessionFile::agent` | interactive session/pane/PID matching; `busy`, `waiting`, `idle`; session-file mtime exposed as whole-second `since` | no completed-turn id or ordered completion observation. An idle session-file rewrite or `/clear` is not proof that a delivered close-out finished |
| Claude fixture tests `finds_only_the_live_interactive_instance_of_this_pane`, `streams_new_lines_and_reports_status`, `a_new_session_id_resets_to_its_transcript` | ignore dead/background/other-session instances; suppress state at foreground shell; reset transcript on session-id change, including a new idle session before transcript exists | need generation-bound observation baseline and a new completed turn, not only an idle state or text/file existence |
| `agents/codex.rs`, `turn_event` and `Conversation` | `task_started` → running with `started_at`; `task_complete` → idle with `completed_at`; `turn_aborted` → idle without a completion timestamp | normalized `Agent` drops native `turn_id` and ordinal. Completion time is whole seconds, so a same-second old/new event cannot be distinguished from the summary |
| Codex fixture `tests/data/codex/rollout.jsonl` | raw start/complete events contain the same `turn_id` and ordered `ordinal` | native ordering/turn data is available in this fixture but not exposed by the adapter interface; prove supported native versions before depending on it |
| Codex fixture tests `normalizes_the_sample_rollout`, `turn_events_set_the_state`, `finds_the_newest_rollout_in_the_panes_directory` | timestamped completion, undated abort, rollout cwd/start-time binding | native startup attempts made no model turn; they are not evidence of a real new-turn close-out |
| `loops/step.rs::finished_turn` | idle/done with `since >= armed_at`; excludes missing timestamp | loop eligibility is weaker than close-out completion; `>=` cannot distinguish events in the same second or bind a generation |
| `task_records/handoff.rs` | existing startup/restart readiness and pending-prompt machinery; user-activity checks | no close-out generation/turn identity. Close-out may deliver to an already idle/done current run, but cannot treat that old state as completion |
| `records/store.rs`, proposed ADR 0024 | per-Task writer/operation groundwork | memory needs a canonical-path workspace lock, monotonic revision and prepared apply journal shared across independent Tasks; a Task lock alone is insufficient |
| `docs/files.rs::read_bytes` | contained read-only listed documents with a 2 MiB cap and lossy UTF-8 rendering | not a mutation/snapshot primitive: path canonicalization followed by open has swap races; lossy UTF-8 and the general Reader cap do not satisfy exact 64 KiB memory bytes |

Both new M0 fixture probes passed locally. The probes in `agents/codex/tests.rs` document two limits of the existing normalized adapter: different native turns completed in the same second produce equal state summaries, and abort/undated completion yields idle with no completion timestamp. These are counterexamples, not production close-out tests. Do not “fix” either by inventing a fresh timestamp or assuming assistant text means completion.

## Ordered observation contract to settle before M1

A close-out delivery stores Task/run, process/native-session identity, generation and an adapter-specific observation baseline **before** dispatch. The completed observation must belong to the same run/session, follow that baseline and the delivered close-out turn, and be a successful completed turn. A later running/waiting/error/aborted state blocks readiness. Raw transcript position alone is not durable across replacement/truncation: bind file/session identity and reset detection as well as offset/ordinal/turn id.

For Codex, probe turn-id and ordinal continuity, duplicate/out-of-order events, truncation, `/clear`/new-session and aborted turns. A `task_complete` carrying a different turn id is not enough unless that turn is bound to this delivery. For Claude, prove an equivalent completed-turn marker in supported native output or add a conservative adapter observation path. Busy → idle seen after delivery may be useful evidence but is not yet accepted as sufficient: polling can miss a fast turn, session-file rewrites lack turn identity, and another user turn may intervene. If a native adapter cannot establish completion, expose failed/not-ready and Terminal; do not offer Apply based on file existence or a timestamp alone.

A user typing during close-out invalidates the delivery/completion association unless the native turn can be identified unambiguously. Restart/session change invalidates the generation. Reading an old completed proposal must never make a new generation ready. Waiting for readiness is bounded to 30 seconds; a timeout cannot later type silently. Delivery claims need explicit uncertain-outcome handling across daemon/fallback senders, as in ADR 0024, without requiring a named worker.

## Snapshot and apply transaction boundary

Use Task → canonical workspace-memory lock order. Two workspace names resolving to the same memory path share revision, journal and lock identity. Capture complete bytes, existence, hash and workspace revision together **after** prepared-journal recovery; snapshot reads at Task creation/restart/close-out and revision-bearing proposal reads must recover or return busy/conflict. Missing differs from empty. Missing live memory removes an old snapshot; invalid/oversize memory fails before an agent replacement. Never truncate or decode with replacement characters.

Close-out proposals use a generation-specific path, then freeze validated completed bytes in bridge-owned state. Review returns that generation’s original complete base and frozen proposal identity, never a silently rebased diff. Apply revalidates Task/run/generation, live base existence/hash, workspace revision and frozen proposal hash under both locks. Stage and flush journal, backup and new file; atomically rename and sync the parent directory; persist committed revision; reconcile the Task result without reversing lock order. The live file and Task record are not one atomic filesystem object, so readers must participate in journal recovery.

Recovery must not acquire a different Task while holding the workspace lock. If the prepared journal belongs to another Task, release locks, acquire that Task first, reacquire workspace, verify the journal identity and recover, then retry the original operation from a fresh revision. If lock ownership or content changed, return recoverable busy/conflict. No automatic editor retry or generation rebase. Any unexplained live content blocks recovery.

Backup retains complete previous bytes when present, newest 20; a missing base has no invented empty backup. Journal identity is Task plus generation and workspace revision, not just bytes: A → B → A and identical proposals from different generations still advance revision. Retrying the same committed operation returns its receipt without another replacement/backup; a newer committed revision conflicts. Local locks coordinate muxa writers, not external editors or same-user agents ignoring them.

## Executable acceptance matrix

These are named integration cases to implement with M2/M3 against real file operations and two independent bridge processes. They are **pending**, not executable memory APIs today. Use temporary workspace roots, synthetic adapter events, explicit barriers and process termination at the named boundary. Assert ids/revisions/hashes and marker counts; do not retain memory, diff, instructions or transcripts in logs.

| Case | Setup / controlled boundary | Required observable result |
|---|---|---|
| M01 old readiness | current run idle/done before request, valid proposal file preexists | prompt may be delivered once; old state/file cannot make ready |
| M02 ordered new completion | same-second old/new completions; new valid turn, then newer running/waiting/abort | only the generation-bound completed turn enables ready; newer active/failed state blocks it |
| M03 run/session reset | replace process or native session after dispatch, retain tags/transcript/proposal | generation invalidated; no old proposal review/apply |
| M04 sender concurrency/crash | daemon and fallback claim one generation; kill winner before/during/after typing | at most one authorized attempt; uncertain delivery never auto-retypes; explicit retry allocates a new generation |
| M05 strict file cap | 65536/65537 bytes, multibyte boundary, NUL, invalid UTF-8, FIFO/directory | exact valid bytes accepted; others fail without hang/truncation or agent replacement |
| M06 missing versus empty | snapshot an existing file, remove live file, then snapshot again | stale snapshot removed; existence/hash/revision coherent; empty regular file remains a distinct base |
| M07 snapshot after rename crash | kill apply after live rename before revision commit; request create/restart/close-out/proposal read | recover journal or refuse; never expose new bytes paired with old revision |
| M08 shared workspace writers | two Tasks/names alias one canonical memory path; simultaneous apply from one displayed revision | one replacement/backup/revision wins; other conflicts, including after winner releases lock |
| M09 generation/run conflict | new close-out/restart/discard races with Apply | Task lock serializes; wrong generation/run/revision cannot apply or mark Done |
| M10 journal crash matrix | kill before journal flush, before/after backup, before/after rename, before/after commit and Task update | retry reconciles exact operation once or conflicts; no duplicate backup/replacement |
| M11 byte equality and ABA | apply equal bytes from newer generation, or restore earlier bytes at newer revision | stale operation retry conflicts despite equal content |
| M12 edited base/proposal | editor changes live base after review; agent edits output after freeze | live-base mismatch conflicts; reviewed frozen bytes never change; no automatic rebase |
| M13 path swaps | final symlink, symlinked `.muxa`/generation/backup parent; swap directory during open/rename | refuse escape using descriptor-relative/no-follow operations; outside marker unchanged |
| M14 recovery lock order | journal belongs to Task A; Task B requests snapshot while A recovery/restart owns guard | no workspace → Task acquisition; bounded conflict/retry, no deadlock or mixed revision |
| M15 independent capabilities | default-agent Task with only workspace-memory; workers-only host; neither | each feature works only with its own capability; no implicit named-worker dependency |
| M16 UI identity/reconnect | pending/failed/ready generation reconnect; unknown future state; failed Apply followed by Done | restore displayed identity; no Apply on unknown/not-ready; failed Apply never implicitly marks Done |
| M17 log and storage hygiene | run success/failure/crash with synthetic sensitive markers | no content in logs; staged/journal/frozen/backup files private and bounded; no live file changed without explicit Apply |

M1 must freeze additive protocol types/fixtures for generation/run, base existence/hash, proposal hash, workspace revision, operation receipt and recoverable state. Unknown close-out states have no active Apply controls. No optional field may collapse missing base into empty bytes. All Rust/Swift/Kotlin codecs share fixtures before endpoints or UI are enabled.

## Checks available now

```sh
cargo test -p muxa-bridge --bin muxa-bridge agents::codex::tests::completion_state_does_not_distinguish_same_second_native_turns
cargo test -p muxa-bridge --bin muxa-bridge agents::codex::tests::abort_and_undated_completion_are_idle_without_completion_time
cargo test -p muxa-bridge --bin muxa-bridge agents::claude::tests::a_new_session_id_resets_to_its_transcript
cargo test -p muxa-bridge --bin muxa-bridge agents::codex::tests::turn_events_set_the_state
```

Run native completion probes separately in a controlled authenticated Task only when a model turn is authorized. Record version, session/process identity, observed start/completion ordering and pass/fail/unknown; no account details or conversation content. Current startup-only probes do not fulfill this gate.

## Status

Source and fixture audit complete; memory execution, journal and close-out completion remain unimplemented. M0 is **not complete** until supported native completion observations and the precise journal/recovery implementation are proven. These findings can inform M1 contract preparation while W proceeds, but cannot enable `workspace-memory` or relax Workers’ release gates.
