# 0024: Serialize Task restart and make uncertain outcomes explicit

## Status

Proposed. Refines ADR 0018 and ADR 0022 for release W. The developer authorized this design and its verification preparation; restart staging, durable dispatch/delivery state, explicit recovery API and clients are now implemented for workspace-default Tasks. Named-worker integration and the remaining native/CI/release acceptance cases are pending. No named-worker execution capability is enabled. The worker-selection contract is still capability-gated.

## Context

Today restart reads a record, replaces TASK.md/HANDOFF.md, respawns, and updates the run in separate steps. The record lock only protects one read/compare/write and deletes locks older than ten seconds. Two writers can both replace a process before one loses the record CAS. A crashed prompt sender can also leave delivery uncertain. Neither an in-process mutex nor routing most requests through a daemon covers independent fallback bridges.

## Decision

### One Task lock for all participating writers

Use a persistent, private lock file per Task under the canonical state directory and an exclusive nonblocking OS advisory lock (`rustix::fs::flock`). Hold its file descriptor in a guard with close-on-exec. Never unlink, replace or expire the lock file; removing a locked inode would allow another writer to lock a different inode. The OS releases ownership when the last owning descriptor closes, including process death. Do not inherit the descriptor into tmux or native agents. Restrict the first release to local filesystems with tested advisory-lock behavior; network/shared filesystems are not supported for lifecycle claims.

All record mutations use this same guard: status/run updates, restart, close/session deletion, loop transitions and check/prompt delivery, and future memory invalidation. Resolve state-directory aliases to one canonical location. Lifecycle acquisition contention returns `conflict`; never wait behind another operation and then execute stale intent. Ordinary short record updates preserve their existing bounded two-second wait and reread/CAS under the OS guard; they never dispatch external effects after that wait. Internally, read/update-under-guard APIs avoid reacquiring the same lock with a second descriptor. Replace the existing stale-file lock instead of nesting a new lifecycle lock around it. No filesystem or tmux work runs on the async executor while performing blocking file operations. Own the guard in the operation task, not a detached worker that can outlive cancellation. Cancellation after durable dispatch intent leaves recovery-required state.

Lock order is Task → workspace when memory is added. Never take a Task lock under a workspace lock. No process holds multiple Task locks. Read-only lists may read atomically replaced records without the guard and expose an in-progress operation; they do not recover it as a side effect.

This is cooperative exclusion between upgraded muxa processes, not exclusion against manual tmux commands, native agents or external editors. Old bridge versions do not honor this protocol. Worker execution requires a coordinated binary/daemon upgrade with old bridges drained; mixed-version concurrent writers are unsupported. W1's downgrade guard is useful but does not make an old writer safe.

### Durable operation state belongs in the Task record

Keep the operation in the same atomic record replacement as its revision. This avoids a separate journal/record commit pair. A private staging directory contains prepared file bytes, never executable recovery commands. Use stable operation ids, schema version, source run id, pane/session ids, pane PID and process-start identity when available, expected revision, reserved next run id, intended assignment, and target directory identity. Store phase, file hashes/existence, staging references and any confirmed replacement identity. Do not store raw command arguments, environment values or instruction text in the record or logs. Staging is mode 0700, files 0600, contained and no-follow checked. Each old/new TASK.md and HANDOFF.md is bounded at 64 KiB, regular UTF-8 without NUL; reject rather than truncate notes or facts. Stage manifests store SHA256, byte count and existence separately, plus anchored directory identities.

Unknown operation versions/phases block mutating operations and automatic senders with a recoverable error. They never mean “no operation”. Operations bump the ordinary record revision, including transitions and resolution. A successful final commit updates run, assignment, pending prompt and operation phase together. Keep a bounded last-operation receipt so an explicit recovery retry with the same operation id can return its prior result; a newer operation/revision conflicts.

The persisted lifecycle follows these phases. In the implementation, finalized is represented atomically by the new run and pending prompt after removing the active operation; resolved is a private receipt, not an active phase exposed to clients:

| Phase | Meaning after owner death | Automatic behavior |
|---|---|---|
| prepared | intent and staged files are durable; dispatch was not authorized yet | no dispatch; explicit cancel can discard stage |
| dispatching | durable authorization precedes the tmux write; command may or may not have executed | recovery required; no retry, file publication or prompt |
| replaced | tmux success and the new process identity were durably recorded | recovery required; explicit finish may reconcile staged files and finalize the reserved run |
| finalized | run/assignment and file publication complete, prompt pending | eligible for a separately claimed delivery using fresh run evidence |
| delivering | durable delivery claim precedes typing any bytes | on owner loss, delivery uncertain; never automatically send again |
| resolved | bounded receipt of cancel/finish/leave-unconfirmed or delivery result | no action from the receipt |

A dead owner with `finalized` can resume the *wait*, not an already claimed delivery. Live ownership blocks recovery even if the phase is old. Age alone never grants ownership or authorizes dispatch.

### Restart order

1. Load/validate worker and workspace config once; read bounded instruction bytes; resolve explicit command and current environment. Prepare content in memory without altering managed files, Task status, loop or process. Worker-aware requests require displayed `rev`; explicit selection/clear without it is bad-request. Legacy omission resolves Keep only after acquiring the guard.
2. Try the Task guard. Reread the record, reject unresolved operations, validate displayed revision, status, source run and pane/process identity, and force confirmation. Take a fresh snapshot for header/handoff generation. Suspend loop/prompt side effects while the operation owns the Task. Validate file identities and preserve user-authored Task notes.
3. Stage headers/handoff privately and persist `prepared`. Flush stage files and their directory, then flush/replace the record and sync its parent directory. Preflight failure before this step has no visible Task-file/process changes. An owner that fails before dispatch can cancel its own prepared operation and restore loop bookkeeping; its revision still advances.
4. Immediately recheck source identity, force/readiness conditions and file base hashes. On mismatch cancel before dispatch. Persist and flush `dispatching` **before** writing a tmux command. Use the selected explicit command; do not reuse an old pane_start_command. Never replay after a timeout, connection loss, cancellation or ambiguous error.
5. A definite success requires a successful tmux reply and a new pane process identity in the same pane. Persist `replaced` before publishing files. A generic tmux error is not proof the old process survived: an exec failure could follow termination. Only failures proven before dispatch are safely cancelled; uncertain outcomes remain blocked for recovery.
6. Publish staged TASK.md/HANDOFF.md with a journaled per-file old/new hash check and durable renames. These two files are not an atomic pair. Until finalization no automatic sender may read/type a prompt; native roles must not auto-start work. A manual reader may briefly see mixed files. If any base changed, leave recovery required; never overwrite edited notes. Once both files match their staged hashes, atomically finalize the reserved run, requested assignment and bound pending prompt. No fresh adapter observation means unknown observed agent/state.
7. Release the guard while waiting for fresh adapter readiness, then reacquire for delivery as below. Notify record subscribers on durable transitions. Return success only after finalization; uncertain outcomes return a safe recovery-required error with operation id and refreshed record state.

Checking PID is not a cross-system atomic compare-and-respawn. The Task lock prevents another muxa writer from changing it between check and dispatch. A manual tmux replacement in that last interval cannot be ruled out; do not claim protection against uncooperative changes. R04 covers a controlled change *before* the final check. Any stronger guarantee needs a separately verified conditional tmux primitive and repro.

### Prompt delivery uses the same Task guard

A pending prompt is bound to operation id, run, pane and new process/session evidence, never only state age. Retained pane tags, old transcripts or timestamps do not satisfy freshness. Process replacement invalidates active-chat binding and state signatures; adapter evidence must identify the new process before readiness is accepted. Waiting/error/unknown and foreground shell are ineligible. Existing user-activity and timeout guards still apply.

After waiting outside the lock, reacquire it, reread pending identity, verify current run/process, and persist `delivering` before typing. Hold the guard through typing and durable result bookkeeping. A partial send or owner death leaves delivery uncertain; the daemon/fallback path cannot retry automatically. A definite send records sent identity, clears the pending prompt and arms the loop in one guarded record mutation. A definite pre-send failure can remain pending only if zero input was dispatched; otherwise require explicit recovery. This gives one authorized attempt, not exactly-once delivery. Restart/Done/close/loop operations cannot race through a delivery claim.

### Explicit recovery, no automatic replacement

Recovery starts with read-only inspection: operation id/phase, current record revision, expected/current process identities and file reconciliation status. List/detail shows requested assignment as unconfirmed during uncertainty. Worker-aware clients disable ordinary restart and automatic actions until resolution. No recovery button may imply that an observed native role was confirmed from the requested worker label.

Implemented additive-v1 request: `recover-restart {task, rev, operation, action}` with actions below. The shared Rust/Swift/Kotlin fixtures are in `protocol/fixtures/restart-recovery/`; availability is advertised independently as `task-restart-recovery`. The CLI first shows inspection, then requires an explicit action with the inspected revision and operation id. All actions acquire the Task guard and revalidate.

- **Cancel prepared:** allowed only in `prepared`, without dispatch authorization; resolve the intent and preserve the previous run and files. Stop automatic pending work; remove private staged files only when safe. Cleanup after the durable resolution is best-effort, so edited live notes cannot trap a cancelled operation. A leftover stage without a durable operation is collectible and never authorizes recovery execution.
- **Finish recorded replacement:** allowed only with durable `replaced`, matching new process identity and a reserved run not already used. Reconcile each file only if its bytes match the expected old or staged new hash. Then finalize once, but suppress automatic prompt delivery; the user opens Terminal to continue. A missing process, edited file or dispatching-only phase conflicts. Do not relaunch or infer success from a different PID alone.
- **Leave unconfirmed:** for uncertain dispatch/delivery or failed reconciliation. Do not send input, respawn, restore old files, assign a confirmed worker or resume the loop. Retain a durable unresolved-outcome receipt and partial-file metadata, clear automatic pending work, and label the latest execution unconfirmed in list/detail/history. Record an interrupted/unconfirmed reserved run where replacement outcome is unknown; close the source run at an unknown boundary rather than presenting it as the confirmed current process. Subsequent restart requires a new explicit request, refreshed identity/revision and normal force confirmation. It may replace the current process but never serves as an implicit retry of the uncertain operation. For delivery-only uncertainty the finalized run/assignment remain, with delivery marked uncertain.

Recovery is idempotent by operation id and action receipt, not by matching bytes alone. No action executes a saved command, reruns a native turn or automatically retypes a prompt. Interrupted/unconfirmed history and recoverable operation summary require additive wire types and UI before enabling workers; legacy clients cannot provide this recovery workflow. Recovery after storage errors may require fixing permissions/free space first; do not remove an operation file or reset a revision by hand.

## Verification and release gates

`crates/muxa-bridge/tests/lifecycle_lock_probe.rs` tests two independent OS processes, directory aliases, ownership beyond the existing ten-second stale threshold, and lock release after forced owner death without unlinking the inode. It is component evidence only; Linux/macOS CI results are needed.

Production acceptance uses two bridge processes sharing one state directory, the daemon/fallback combination, synthetic workers and isolated tmux. R01–R10 in `docs/findings/workers-w0.md` remain mandatory. Add these cases:

| Case | Boundary | Required assertion |
|---|---|---|
| R11 | status update, close or loop sender while restart/delivery owns guard | conflict/defer, no close, process replacement or input; no deadlock |
| R12 | kill owner in prepared/dispatching/replaced/finalized/delivering | persistent phase determines permitted action; never an automatic respawn or duplicate send |
| R13 | kill between TASK.md and HANDOFF.md renames | no prompt; explicit finish reconciles once or conflicts on edits |
| R14 | retry recovery after success; then after a newer operation | same receipt without extra run/file write; newer rev conflicts |
| R15 | invalid/unknown operation, edited stage, alias paths, lock older than ten seconds | safe blocking/containment; one owner, no stale-lock deletion |
| R16 | retained old tags and old active-chat source after process replacement | new run unknown, old session never attached, no prompt until fresh evidence |

Fault hooks belong only in the test harness at these named boundaries. Do not add general production fault injection. Count native replacements and typed input using synthetic marker files; assert pane/layout unchanged, run count, assignment, revisions, file hashes and absence of secrets from logs. Run the same request through daemon and fallback paths. Component lock tests and model-only traces cannot close these integration gates.

## Consequences

This changes the record writer API and restart/prompt/loop coordination before worker execution can be enabled. It deliberately exposes uncertainty instead of claiming a filesystem/tmux transaction or exactly-once delivery. Storage is durable before external effects, but arbitrary editors and manual tmux operations do not participate. Recovery UI/wire is implemented as independent W2/W3 scope; memory keeps its separate workspace journal and Task → workspace order.

## Fresh observation after replacement

Finalization stores `restartObservation` privately on the new run, in the same record update as its pending prompt: replacement PID/start identity, session/pane, operation id and the earlier adapter session/transcript. It is not a wire field or confirmation of the requested native role. Unknown or malformed observation metadata blocks automatic input.

For these new runs, a retained tmux state/session tag is not readiness. The adapter must associate its reported session with a process currently inside the replacement pane, and the session/transcript must not reuse the source observation. The input sender reacquires the lifecycle claim, observes the adapter again and checks the exact process-start identity before persisting delivery. A later restart applies the same filter before force confirmation or carrying observations into history. An unconfirmed replacement requires explicit force before another restart.

Claude’s session files include a PID checked against the current process tree. Codex currently matches rollout files by cwd and approximate process-start time; it supplies no process-bound evidence for this path. New managed Codex restarts and hook-only custom agents therefore wait without automatic continuation, then show the fixed prompt for manual Terminal input. Manual sessions and baseline-free legacy waits retain their existing behavior. Authenticated native selection and stronger Codex binding remain release gates.

## Local implementation evidence

`task_records/restart.rs` stages through descriptor-relative `records/restart_files.rs`, validates source PID/start identity immediately before dispatch, and persists uncertainty before writing to tmux. `handoff` and loop senders persist delivering before typing; legacy Tasks with no run use an explicit absent run binding rather than invented run history. Action receipts commit in the same record replacement as their resolution. `ps -p PID -o lstart=` supplies the stable process-start identity; failures remain unconfirmed.

Bridge/daemon recovery cases and four real-bridge kill boundaries (pre-dispatch, post-replacement/pre-reply, pre-input, partial-input) now have executable tests in `tests/task_records.rs`. Kernel-owner-death and staged-file safety tests remain separate component evidence. Named worker role selection, stale worker-aware restart revisions, fresh adapter/chat binding and remaining release cases must still be verified before advertising workers.
