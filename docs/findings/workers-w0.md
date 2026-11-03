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
- `sessions.rs` adapters still fill stateless panes; retained tagged state prevents that fallback. `states.rs` signatures omit PID and state timestamp, so worker-native rematching/chat freshness remains gated. A live claim is not fresh native-agent evidence.

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

R01–R10 remain the named-worker release acceptance matrix. Default-agent production paths now cover selected concurrency, uncertainty and recovery boundaries as described in ADR 0024 and `docs/workers.md`; do not count those as completion of every worker case. The W0 component probes above do not substitute for them. ADR 0024 specifies the proposed lifecycle-claim storage/lock order and explicit recovery operation. W2 must make these cases executable against that boundary, including R11–R16 from the ADR, before enabling workers. Do not add production fault injection before that design is accepted.

## Native startup attempts

A follow-up probe reran the two isolated tmux tests and the registry/CAS unit tests successfully. Installed versions remain tmux 3.7c, Claude Code 2.1.316 and Codex CLI 0.166.0.

Startup-only native attempts used a temporary non-repo Task directory, newly authored native definitions, temporary config directories, an environment without inherited provider credentials, and no positional prompt. No Task prompt or model turn was sent. Terminal output was inspected only in memory and was not retained in logs.

- Claude: startup/readiness was not established within the bounded PTY probe. Process-group termination also encountered an execution-environment permission error. This is inconclusive, not evidence that the native role is unsupported.
- Codex: the controlled profile containing `sandbox_mode = "read-only"` and `approval_policy = "untrusted"` exited with a configuration error. The specific cause is unclassified; those settings are not accepted as startup evidence. A missing profile reference reached onboarding rather than establishing a missing-profile error. A comment-only valid profile also reached onboarding with no configuration error. Authentication/onboarding may precede native-reference validation: never treat reaching that screen as proof of profile selection or fallback behavior.

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

The automated component facts are established locally. W0 as a whole is **not complete**: native startup/discovery/rematching, cross-version results and executable production restart/recovery acceptance gates are still outstanding. Lifecycle claim/recovery from ADR 0024 is implemented for default-agent Tasks, including staged-file and real bridge-kill tests. This is not acceptance of all named-worker/native release cases. The developer subsequently authorized W1 contract/types/fixtures while these W0 gates remain open. Named-worker execution remains gated; restart recovery is independently available through `task-restart-recovery`; parallel W2/W3 foundations and client preparation were subsequently authorized and are tracked in `docs/workers.md`.
