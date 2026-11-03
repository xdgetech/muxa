# Workers W0: evidence and remaining gates

ADR 0022 and `docs/workers.md` remain Proposed. This report distinguishes tests of existing components from acceptance of a future worker implementation. W0 introduced no worker capability or lifecycle claim. W1 subsequently added contract-only wire fields; worker execution remains disabled.

## Automated evidence

Local tmux: 3.7c. All tmux tests use throwaway `muxa-test-*` servers. Fake workers write synthetic argv/environment/PID fixtures to temporary files; their content is not added to application logs.

| Evidence | Executable check | Result and limit |
|---|---|---|
| Explicit worker command replacement | `scripts/repro/worker-respawn.sh`, `worker_commands_respawn_in_place_with_exact_arguments` | default → A → B → edited-A command → default executes in the same pane; each replacement changes the process; special-character/UTF-8 arguments arrive byte for byte; workspace env survives. The public explicit-command API now covers both a single shell command and direct argv, with literal tilde encoded to avoid tmux expansion. This proves explicit command replacement, not end-to-end worker resolution/runtime |
| Repeating a selected command | same test, no-command respawn after **every** explicit selection | the most recently explicit command executes again with the same argv/environment; no reliance on tmux's escaped display representation of pane_start_command |
| Desktop preservation | same test, real PTY client viewing another window | pane id/size/layout, session selections and control/desktop client views remain unchanged throughout the sequence |
| Old hooks survive replacement | `worker_respawn_retains_old_hook_tags` | pane PID changes but state `waiting`, timestamp `1` and the old session-id tag remain. Existing discovery still returns the stale tagged state. That is baseline evidence, not worker readiness |
| Registry can reselect per snapshot | bridge unit test `registry_rematches_replaced_processes_in_the_same_pane` | fixture adapter A → B → A in one pane follows changed PID snapshots; an unmatched process/shell has no match. This does not prove native adapter recognition or active-chat rebinding |
| Record CAS is insufficient around external effects | bridge unit test `record_lock_does_not_serialize_external_restart_effects` | two independent Store handles synchronize before synthetic external effects; both effects occur, then one record update wins and one conflicts. A successful record lock does not serialize earlier process replacements |

Run the focused checks with:

```sh
bash scripts/repro/worker-respawn.sh
cargo test -p muxa-bridge registry_rematches_replaced_processes_in_the_same_pane
cargo test -p muxa-bridge record_lock_does_not_serialize_external_restart_effects
```

Local verification passed: `make check`, `make test-tmux` and `make repro`, including the new worker probes. No native model turn was run.

The new repro must also pass the supported tmux CI matrix before the cross-version gate is complete. The local result establishes only tmux 3.7c.

## Current bridge audit

The baseline audit below motivated ADR 0024; subsequent implementation now provides persistent OS locks and nonblocking `Store::claim`, rather than ten-second stale-file removal.

- `task_records/restart.rs` now owns one Task claim through file writing, respawn and run save. This serializes participating live restart writers, but files are now privately staged before dispatch, with durable prepared/dispatching/replaced intent and source/new process-start identity. Publication occurs only after confirmed replacement; explicit recovery reconciles partial files or preserves uncertainty.
- `records/store.rs` canonicalizes the state directory, preserves lock inodes, refuses final lock symlinks and syncs record directory changes. Ordinary short record updates retain the existing bounded wait; lifecycle claims fail immediately on contention. Unknown `restartOperation` data blocks ordinary mutations. Production restart and input senders now persist operation intent; unknown phases still block mutation and automatic input.
- `handoff.rs` now claims pending delivery and holds the guard through input and bookkeeping. `loops` guards input, check process dispatch and header refresh; linked session close acquires the same claim. Pending removal, observed-run bookkeeping and loop arming now commit in one guarded record revision. Startup/restart/loop delivery now has durable uncertainty and explicit resolution. Interrupted-check and named-worker/native observation combinations still need the remaining release acceptance coverage.
- `sessions.rs` keeps tag precedence for manual/legacy panes. Managed restarted runs now pass through `task_records/observation.rs` after enrichment: it discards tag state and accepts only a process-bound adapter session matching the run’s replacement identity. Missing records, unconfirmed outcomes and invalid observation metadata stay unknown. `states.rs` signatures now include PID and state timestamp. `chat.rs` discards the old source and reopens through the registry when its pane PID or tmux session changes; fixtures and a real-bridge replacement test cover the same subscription receiving a reset from the new transcript. Native rematching, child-agent replacement within an unchanged pane PID and fresh run readiness remain gated. A live claim is not fresh native-agent evidence.

## Lifecycle lock and recovery design

[ADR 0024](../adr/0024-task-restart-lifecycle.md) now specifies a persistent OS-lock inode shared by all Task writers, operation phases stored atomically in the Task record, guarded prompt delivery, and explicit cancel/finish/leave-unconfirmed recovery. It replaces age-based lock removal and documents the coordinated-upgrade requirement and manual-tmux race limit. It remains Proposed. Production now uses the OS record lock, staged restart, durable delivery and explicit recovery for default-agent Tasks; remaining native/worker release cases are pending.

`cargo test -p muxa-bridge --test lifecycle_lock_probe` passed locally: an independent process cannot acquire through either directory alias while the owner lives, including after eleven seconds; killing the owner releases the same inode for a new process. This proves the selected local OS-lock primitive, not restart serialization, descriptor inheritance handling in production, or crash-safe operation persistence. The existing CI runs this test on Linux/macOS through `make check`.

## Restart acceptance cases to implement with W2

These are concrete integration cases, not claims that today's worker API exists. Use two bridge processes sharing one state directory and barrier/fault hooks at the listed boundaries. Only synthetic markers, ids, revisions and process identity belong in traces.

| Case | Controlled boundary/input | Required observable result |
|---|---|---|
| R01 stale displayed rev | advance the record before request/preflight | conflict; unchanged process, brief/handoff and run count |
| R02 simultaneous requests | hold first claim; send second request through another writer | second conflicts before any process/file change; one replacement and one new run |
| R03 second request after winner | first request completes; second uses the original displayed rev | conflict, with no second replacement |
| R04 changed pane/run | replace the source process after preflight, before final identity check | conflict; the stale request does not kill the new process |
| R05 invalid instructions | missing/oversize/invalid UTF-8 file | no claim side effect, process change, loop change or published header |
| R06 definite tmux refusal | deny the replacement command before execution | previous process/run/assignment/files authoritative; staged files discarded |
| R07 crash before side-effect outcome known | kill the bridge after durable intent, around command dispatch | retain uncertain operation; no automatic replacement or prompt retry after recovery |
| R08 crash after respawn | kill before file publication or run finalization | new process is not described as confirmed old/new worker; requested/unconfirmed UI and explicit recovery path |
| R09 stale state/session tags | retain old tags/transcript paths while replacing A with B | no old state/session bound to B's run; fresh adapter evidence required before the fixed prompt |
| R10 same-name edit/default clear | edit A's command, restart A, then select Workspace default | new configured command each time; explicit run assignments; unchanged pane and desktop layout |

R01–R10 remain the named-worker release acceptance matrix. Internal selection/staging tests now check stale/missing revision, invalid instructions, immutable instruction snapshots, same-name command edits, default clearing and note preservation. An internal real-tmux/fake-agent test additionally exercises create, same-name restart and default clear with native argv/environment, explicit assignments and unchanged pane/session/layout. These do not execute the enabled wire path or establish authenticated native readiness. R02/R03/R04/R05 now also have internal real-tmux/fake-agent concurrency, stale-winner, changed-source and invalid-instruction cases. A generic tmux-error case confirms uncertain dispatch blocks retry rather than claiming safe refusal. R16’s default-agent path now retains a new-run baseline, rejects retained tags/old sessions, checks strong adapter process ownership and rechecks PID/start before input; same-second tags, fresh replacement sessions, follow-up Running confirmation and unknown forced restart are exercised through bridge/daemon tests. This is not authenticated native-role acceptance. Codex’s cwd/time heuristic is intentionally not strong ownership evidence for a guarded run. R11 competing status/loop/recovery writers and R15 unknown operations/tampered or symlinked stages have additional bridge/daemon coverage. Default-agent production paths now cover selected concurrency, uncertainty and recovery boundaries as described in ADR 0024 and `docs/workers.md`; do not count those as completion of every worker case. The W0 component probes above do not substitute for them. ADR 0024 specifies the proposed lifecycle-claim storage/lock order and explicit recovery operation. W2 must make these cases executable against that boundary, including R11–R16 from the ADR, before enabling workers. Do not add production fault injection before that design is accepted.

## Native startup attempts

A follow-up probe reran the two isolated tmux tests and the registry/CAS unit tests successfully. Installed versions remain tmux 3.7c, Claude Code 2.1.316 and Codex CLI 0.166.0.

Startup-only native attempts used a temporary non-repo Task directory, newly authored native definitions, temporary config directories, an environment without inherited provider credentials, and no positional prompt. No Task prompt or model turn was sent. Terminal output was inspected only in memory and was not retained in logs.

- Claude: startup/readiness was not established within the bounded PTY probe. Process-group termination also encountered an execution-environment permission error. This is inconclusive, not evidence that the native role is unsupported.
- Codex: the controlled profile containing `sandbox_mode = "read-only"` and `approval_policy = "untrusted"` exited with a configuration error. The specific cause is unclassified; those settings are not accepted as startup evidence. A missing profile reference reached onboarding rather than establishing a missing-profile error. A comment-only valid profile also reached onboarding with no configuration error. Authentication/onboarding may precede native-reference validation: never treat reaching that screen as proof of profile selection or fallback behavior.

### Codex ownership audit (2026-11-03)

Installed Codex CLI 0.166.0 was checked with `--version`, CLI/app-server/agents help and `app-server generate-json-schema --experimental` into a temporary directory. No app-server runtime, authentication change, configuration copy, startup prompt or model turn was involved. Generated schemas and their temporary paths are not repository artifacts.

- Installed help describes a shared local app-server daemon, a `--no-daemon` option and remote TUI connections. A pane's TUI PID therefore cannot be assumed to own the execution process or its history file. This is a limitation of the existing process-tree/rollout heuristic, not evidence that a particular native launch has been verified.
- The generated `Thread` schema includes `id`, `sessionId`, `path`, `cwd`, `historyMode` and runtime `status`; it exposes no owner PID or client PID field. The schema's `processId` elsewhere describes executed PTY commands, not ownership of the Codex thread. No PID-bound native session file was established by this audit.
- Official App Server documentation distinguishes thread creation from starting a model turn, gives thread/session identities, and says new persistent local threads default to paginated history. It also says resuming alone does not update the thread or rollout timestamp. Release verification must therefore establish the supported native history mode and TUI/execution binding; legacy JSONL fixtures cannot establish either for a current shared-daemon launch. See [App Server lifecycle and history](https://learn.chatgpt.com/docs/app-server#start-or-resume-a-thread).
- Official hook input supplies `session_id`, `transcript_path` and `cwd`; it does not document an owning pane/process PID. The transcript format is explicitly unstable. A generic existing `SessionStart` callback is therefore not itself proof that its session belongs to a particular muxa run. See [Hooks: common input and SessionStart](https://learn.chatgpt.com/docs/hooks#common-input-fields).

**Inventory/cutoff counterexample.** A temporary synthetic-file probe captured all existing candidate ids before dispatch and used a precise timestamp cutoff. The intended replacement started but had not written history. An unrelated Codex in the same cwd then created a new session after that cutoff. Its file was absent from the inventory, newer than the cutoff and the only candidate. A unique-candidate rule selected the unrelated session. The probe reported one candidate and `unique_candidate_is_foreign = true`; it wrote no conversation content and contacted no native agent. More accurate clocks, file birth times and excluding the immediately preceding session do not establish ownership in this schedule. A per-Task lock does not exclude another manual agent in the same directory. Inventory overflow, partial listing or uncertain restart recovery would also have to fail closed rather than treating an incomplete inventory as complete.

Consequently Codex remains `process_bound = false`: guarded new runs cannot derive readiness, Chat ownership or automatic delivery from cwd/mtime proximity, including when the source run has no observed session. Existing manual/legacy inspection remains available. `--no-daemon` alone does not repair this: a rollout still needs an independently verified link to that process, and native behavior has not been probed here.

A possible future minimal contract is a native `SessionStart` receipt carrying a durable operation/run nonce that muxa allocates before dispatch. The callback would atomically publish only operation/run identity, native session/history identity and startup source into private bounded state. Acceptance would require proving that the nonce is scoped to this launch/thread, survives the intended shared-daemon path without leaking to another session, and cannot be satisfied by an old/delayed receipt; process/session changes and ambiguous or missing receipts would suppress input. The native configuration and trust path must be tested without a startup turn. This requires a new ADR and native acceptance, not a new heuristic or an implementation in this slice. A larger alternative is an owned App Server `thread/start` response followed by a TUI attached to that exact returned thread; that would change muxa's launch/attach contract and likewise needs an ADR. Neither proposal requires copying authentication or adding worker-specific credentials.

**Owned-thread probe (2026-11-03, Codex CLI 0.166.0).** A private `codex app-server --listen stdio://` with a temporary `CODEX_HOME`, `daemon_auto_start` disabled and no login answered `initialize` and `thread/start`. No turn was started and no prompt was sent. The response held a thread id, status `idle` and a rollout `path` containing that id. No session file existed on disk afterwards, and `thread/list` was empty. Without `historyMode` the thread was `paginated`; with `historyMode: "legacy"` it was `legacy`. The server logged one failed network connection attempt, as expected without login. Two public reports bear on the receipt design: hooks run inside the shared daemon with the environment of the pane that started it ([herdr #4859](https://github.com/herdrdev/herdr/issues/4859)), and `SessionStart` runs on the first turn, not at `thread/start` ([openai/codex #54353](https://github.com/openai/codex/issues/54353)). [ADR 0028](../adr/0028-codex-owned-threads.md) builds on these facts.

Native selection, adapter binding and readiness remain unverified. The developer can follow [the manual procedure](workers-w0-test.md) in an authenticated terminal. Existing help/version evidence is recorded in `docs/workers.md`.

## Developer-run native startup matrix

Run this matrix using **new controlled native definitions**, not copies of real configuration. Definitions have no initial prompt, startup work or modifying hooks. Keep the actual agent instruction file outside muxa's configuration; muxa only refers to the native role/profile name. Do not put credentials in argv, copy authentication files, or change existing real profiles. If a temporary config home needs login, record that prerequisite and stop; logging in is a separate developer action.

| Probe | Launch directory/config | Check without a task turn |
|---|---|---|
| N01 | repo Task worktree, native user role/profile | intended native selection is visible; adapter sees the actual process; initial state is reported or explicitly unknown |
| N02 | repo Task worktree, project Claude role | native project discovery works from that worktree; no assumption that an untracked main-checkout role exists in the worktree |
| N03 | non-repo `<workspace>/tasks/<fixture>` | native role/profile is discoverable here; record whether the workspace ancestor is searched rather than assuming it |
| N04 | same directories, custom workspace config home | native settings and muxa adapter directory lookup agree; no worker-specific env layer |
| N05 | invalid native role/profile reference | record native error/warning/fallback behavior; muxa must not claim confirmed native-role selection based on its requested worker label |

Before launching, use `--help` to confirm the installed CLI syntax. Run interactively without a positional prompt or `--print` request. Do not use `claude agents` as a role-listing command: this installed version uses it for background sessions. Stop before sending a Task prompt or making a model turn; native onboarding/trust/login screens are not successful adapter/readiness checks.

A native definition might itself initiate work. Review only the newly authored fixture to ensure that does not happen. If checking selection or readiness requires a model turn, leave it unverified and specify the additional developer-run check; do not synthesize `idle`. Record only version, directory/config class, intended selection, observed agent id/state, and pass/fail/unknown—no terminal or transcript content.

## W0 status

The automated component facts are established locally. W0 as a whole is **not complete**: native startup/discovery/rematching, cross-version results and executable production restart/recovery acceptance gates are still outstanding. Lifecycle claim/recovery from ADR 0024 is implemented for default-agent Tasks, including staged-file and real bridge-kill tests for prepared, dispatching, finalized and delivering boundaries. Replaced-phase publication and rename-between-files real-kill verification remains pending. This is not acceptance of all named-worker/native release cases. The developer subsequently authorized W1 contract/types/fixtures while these W0 gates remain open. Named-worker execution remains gated; restart recovery is independently available through `task-restart-recovery`; parallel W2/W3 foundations and client preparation were subsequently authorized and are tracked in `docs/workers.md`.
