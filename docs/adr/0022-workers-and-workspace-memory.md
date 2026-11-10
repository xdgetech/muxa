# 0022: Workers and workspace memory

## Status

Proposed. Implementation plan: `docs/workers.md`. Reviewed with GPT-6 Astra (`docs/findings/workers-design-review.md`). This ADR proposes two independent releases: **W, Workers and handoff**, then **M, workspace memory and close-out**. W1 contract/type/fixture work is authorized while W0 gates remain open. Named create and restart are implemented, and `workers` is advertised since 2026-11-06; M is not implemented. Per-worker environment/account overrides are deferred; workspace environments continue to follow ADR 0016. ADR 0027 simplifies the worker selection contract to a name with reserved `default`, and replaces release M with a Routine.

## Context

A workspace currently selects one agent command and environment (ADR 0016). Tasks have durable records and run handoff (ADR 0015, 0018), but cannot select a reusable worker or retain decisions across Tasks. The developer wants:

1. Named workers with an AI, instructions, allowed workspaces and default checks, referring to Claude Code subagents and Codex profiles rather than duplicating native role configuration.
2. A worker on each Task and a restart that can hand work to another worker.
3. WORKSPACE.md per workspace, with agent-proposed close-out decisions that the user reviews as a diff before muxa applies them.

Task notes stay in TASK.md. Project instructions stay in the repo's CLAUDE.md and AGENTS.md. Workspace memory holds decisions that span repos and work without a repo. Workers do not require separate subscriptions or logins.

### Native roles: evidence and implementation gate

- Codex `--profile NAME` layers `$CODEX_HOME/NAME.config.toml` over the user configuration. Native profile configuration remains owned by Codex. Project and CLI layers may override profile values, so a profile is not an immutable enforcement boundary. See the [official CLI reference](https://learn.chatgpt.com/docs/cli/reference) and [configuration profiles](https://learn.chatgpt.com/docs/config-file/config-advanced#profiles), checked on 2026-11-02. The installed CLI help also confirms the profile-file argument. Do not confuse restrictions on project-local config keys with profile restrictions.
- Claude Code documents `claude --agent NAME` as running the main session with a native subagent's role. A nonempty role prompt replaces the default system prompt. See [subagents](https://code.claude.com/docs/en/sub-agents#invoke-subagents-explicitly) and [CLI reference](https://code.claude.com/docs/en/cli-reference), checked on 2026-11-02. Installed help confirms `--agent` on Claude Code 2.1.316. Discovery from actual repo worktrees and non-repo Task directories, custom CLAUDE_CONFIG_DIR and adapter matching still need controlled runtime probes. Do not assume a workspace ancestor's `.claude/agents` is visible inside every repo.
- Local help confirms Codex CLI 0.166.0's file-based `--profile`. Help/version commands started no conversation. `claude agents` now manages background sessions; it is not a role-discovery validation command for this plan. Documentation/help establish the supported launch interface, not that a particular role exists or successfully starts.
- muxa does not read, rewrite, import or synthesize native agent configuration. The native CLI validates its own role/profile references; errors and warnings stay visible in Terminal. muxa never substitutes another worker, but an assignment label records the requested configuration, not proof of the native CLI's effective role or permissions. The native role owns model, tools and permission settings; muxa owns assignment and Task orchestration. Instructions added to TASK.md are ordinary task context, not system policy.

## Decision

### W1. Host-owned worker definitions

Workers live alongside workspaces in `workspaces.toml`. Clients cannot write this file. Long prose remains in a separate instruction file.

```toml
[[worker]]
name = "Reviewer"
description = "Reviews a branch and records findings."
agent = "claude-code"
args = ["--agent", "reviewer"]
instructions = "~/.config/muxa/workers/reviewer.md"
workspaces = ["Company"]

[worker.loop]
check = "make check"
max_attempts = 3
timeout = 600

[[worker]]
name = "Implementer"
agent = "codex"
args = ["--profile", "implementer"]
workspaces = ["Company"]
```

- `name`: unique, nonempty, at most 64 characters, no controls. It is the persisted identity for this slice. Renaming is removal plus addition; existing Tasks must explicitly select the new name. A display-name/id split can be added separately.
- `description`: optional, at most 200 characters, no controls. ADR 0051 raised the limit to 1 KiB for every worker.
- `agent`: `claude-code` or `codex`; selects the program. It does not assert the adapter's observed state or agent identity (invariant 6).
- `args`: optional, at most 32 arguments, each at most 1024 bytes and without controls. Host-owned flags, not a sandbox or allowlist. muxa-tmux quotes each argument and is the only command-line construction path.
- `instructions`: optional absolute path after expanding a leading `~/`; read on each start/restart, UTF-8, at most 16 KiB. A missing, invalid or oversized file fails before stopping the old agent. Never log its content.
- `workspaces`: optional list of existing workspace names; absent allows assignment in every workspace. This restricts muxa assignment, not filesystem access or tool permissions. The UI shows the scope. Unknown workspace names are configuration errors.
- `loop`: optional defaults within ADR 0019's ranges. The check must be permitted in every workspace where the worker can be assigned, including all workspaces when the list is absent. Defaults prefill the creation form; assignment never starts a loop implicitly or widens the host's allowed checks.
- `env`: unsupported in release W; reject it as unknown configuration rather than ignoring it. Worker-specific accounts require a later environment-reset design. Existing `[workspace.agent].env` remains supported.

A worker command replaces the workspace's `agent.command` or bridge default. muxa adds no worker-specific environment overrides: the launch uses the existing workspace environment path (ADR 0016). A Task without a worker keeps the workspace's default agent. Adapters keep their existing workspace-configured directory search; there are no worker config directories to add. Native settings, profiles, shell startup and plugins may affect the effective environment. This does not promise that two roles have identical accounts or access.

The bridge writes a `## Worker` section in TASK.md's managed header with the selected name and current instruction text. The agent-maintained notes below the marker remain untouched. Native persona/tool rules stay in native role files; muxa does not copy them into TASK.md. Native startup prompts that automatically begin work must be disabled or proven compatible with muxa's fixed first prompt in W0. Worker `args` are trusted host configuration; they must launch an interactive agent in the Task directory, without resuming an unrelated conversation or starting a separate native worktree/background/remote session. muxa does not silently rewrite native settings to satisfy this contract.

### W2. Task assignment and restart

> **Superseded in part by ADR 0027 section 1.** A worker-aware client sends a worker name or `default` (the workspace default agent). A legacy client omits `worker` and keeps the current worker. `worker: null` no longer clears; the CLI uses `--worker default` instead of `--workspace-default`. `rev` is optional and checked only when sent. Runs record `worker` (a name or `default`) instead of `assignment {kind, name?}`; a legacy run without it stays unrecorded. The text below is the original decision.

Records gain optional `worker`; absence means the workspace default. Each new run gets an explicit `assignment`: `{kind: workspace-default}` or `{kind: worker, name: Reviewer}`. Absent `assignment` on a legacy run means unrecorded, not evidence that it used today's default. Do not infer historical assignment from current config. A run's observed `agent` still comes only from the adapter. Resolve command, scope, environment and instructions on **every** restart, including same-name restarts; do not pair newly loaded instructions with an obsolete pane command.

Creation takes optional `worker: NAME`. Restart has three explicit forms:

| Request value | Meaning | CLI |
|---|---|---|
| worker omitted | keep the Task's current assignment | `task restart ID` |
| `worker: NAME` | select that worker | `task restart ID --worker NAME` |
| `worker: null` | clear the assignment; use workspace default | `task restart ID --workspace-default` |

The Rust decoder must distinguish omission from null; Swift/Kotlin encoders and fixtures must preserve the distinction. The two CLI selectors are mutually exclusive. Clearing the assignment removes `record.worker`, but the new run records `assignment: {kind: workspace-default}`. A removed or disallowed current worker is an error when keeping it; never silently fall back.

Before replacing a process, validate the full selection and prepare the header/handoff. Apply ADR 0018's running-agent confirmation and conflict checks. A successful handoff uses respawn-pane in the existing pane without selecting or resizing anything, and records the new run only through the established restart lifecycle. HANDOFF.md names both assignments and retains factual history and prior transcript paths. The prompt remains `Read .muxa/TASK.md and .muxa/HANDOFF.md, then continue.`

**Restart transaction.** Proposed [ADR 0024](0024-task-restart-lifecycle.md) specifies the shared OS lock, durable operation phases and explicit recovery actions for this boundary. Its component probe is not production acceptance.

 Worker-aware clients send the record revision they displayed with `restart-agent`. Add optional `rev`; release-W clients always send it, including Keep current. A stale revision is `conflict` before any files or process change. Explicit worker selection/clearing requires `rev`; omitting both fields retains the legacy Keep path. Use a per-Task exclusive lifecycle claim shared by daemon/fallback writers, and recheck revision and pane/run identity immediately before respawn. A concurrent restart must not stop a newly started worker or overwrite its recorded assignment. Reuse safe pieces of ADR 0018, but do not assume today's check-then-write sender is already serialized.

Preflight reads the chosen worker, permitted workspace, instructions and command before modifying the managed header or terminating an agent. Record a durable operation intent with source run/pane identity, expected revision and intended assignment before the external side effect. Finalize the run/assignment after successful respawn; the new run's adapter identity starts unknown until fresh observation belongs to the new process. Retained pane tags or the previous adapter's transcript do not confirm the new worker. Before-respawn failure leaves the previous run and TASK.md intact; post-respawn failure retains an uncertain intent, suppresses automatic prompt/restart retry and reports a recoverable error. The UI labels the selection as requested/unconfirmed, not as a confirmed running worker. A repeated user request resolves against refreshed state. W0 pins this failure/recovery boundary; no exactly-once process replacement claim across crashes.

**Existing environment limitation.** `new-session -e` values survive respawn (`scripts/repro/session-env.sh`, `scripts/repro/respawn-pane.sh`). Removing a key from workspace config does not guarantee its removal from an existing session. Release W does not add another layer or claim to fix that ADR 0016 limitation. Changing or removing workspace credentials requires a fresh session until an exact reset primitive is designed. Worker handoff cannot change the Task's workspace. Future per-worker env would require tracking managed keys and proving true key removal across A → B → default, without secrets in arguments/logs or user-session option mutations; that research is not a prerequisite for W.

The built-in launcher supports workers first. Use tmux-agents only when its advertised `agent-env` contract also carries the selected command and satisfies the same environment rules; otherwise use the built-in launcher. Do not add `@muxa_worker` writes in this slice. Desktop clients can read `record.worker` from CLI JSON; a tmux metadata cache belongs to ADR 0023 slice 2 and requires its invariant exception.

### M1. Workspace memory and close-out

Release M works with workspace-default Tasks as well as named workers; it does not require the `workers` capability. The optional live file is `<workspace-root>/WORKSPACE.md`. It contains durable decisions and reasons, conventions and links; no Task progress. People can edit it directly. muxa's managed path only writes it after an explicit apply request from a reviewed diff. This is a workflow gate: an agent running as the same OS user may still write reachable files or invoke a CLI. Preventing that requires native permissions or OS isolation; hashes do not establish human identity. Worker scope lists are not a security boundary.

**Complete snapshots only.** On creation/restart, capture bytes, existence, hash and workspace revision together under the canonical-path workspace lock, recover any prepared journal operation before trusting that revision (or return recoverable busy/conflict for an ordered recovery pass), then copy the full valid file into `.muxa/WORKSPACE.md` through temp/rename, maximum 64 KiB UTF-8, no NUL. Missing memory means an absent snapshot; remove a previous run's snapshot rather than keep stale content. An oversized or invalid live file fails memory-enabled start/restart before replacing an agent, with the path and size in the error. Never truncate it into an editable replacement base. Persist that coherent snapshot identity with the run before referring to it from TASK.md; never pair bytes from one workspace revision with another revision number. With both locks needed, take Task → workspace. A standalone snapshot may take the workspace lock alone, but releases it before taking any Task lock. TASK.md names the snapshot and its timestamp. Copies avoid requiring extra sandbox roots but do not prevent same-user writes to the original.

Close-out is an explicit request, offered before Done when an agent pane exists, or from detail. The user can skip it. `update-task` does not start close-out, and close-out does not change Task status. A dropped Task can request close-out manually; no automatic prompt on Dropped in the first slice.

#### Close-out lifecycle

1. Under the Task's existing lock, allocate a unique generation and bind it to Task id, run id, workspace canonical path, and the complete base snapshot/hash. Stop an active loop with reason `user`. Invalidate all prior proposals. Refresh the complete memory snapshot and its revision under that same workspace lock; fail on oversize/invalid data. Store `closeOut {generation, run, workspaceRevision, state: pending, since, base, baseExists}` before scheduling the prompt. Use generation-specific `.muxa/proposed/<generation>/WORKSPACE.md` and instruction paths so an old file cannot become a new proposal.
2. Readiness differs from the start prompt: a believable idle/done state belonging to the current run is eligible even when it predates the close-out request. Running agents may finish their current turn; waiting/error/unknown/shell-foreground states are not eligible. Check the same user-activity guard as ADR 0019. Wait at most 30 seconds, then record `failed` with a safe explanation and offer Retry/Open Terminal. Do not silently type later after failure. Keep generation and run in durable pending state; a restart or run change invalidates it.
3. Under the Task lock, claim exclusive delivery for this generation/run durably before typing the fixed instruction path. The daemon and all fallback bridges must acquire that same claim; no other pending sender may deliver it. Release the lock for delivery only after the claim is durable and honored by restart/discard, then record `sent`, send time and observation identity. A crashed or expired delivery claim where delivery is uncertain becomes `failed` and requires an explicit retry, rather than automatically typing a second close-out. The agent copies the complete base (or starts an empty file for a missing base), proposes only workspace facts with reasons, and ends its turn. Repo-only instructions belong in a reviewed repo change.
4. Expose `ready` only after a **new completed turn** from the same run after delivery, and a validated generation-specific proposal. Do not infer completion from file existence or the pre-request idle/done state. A newer running/waiting state blocks review until the turn finishes. If adapter timestamps cannot distinguish the new turn, remain pending/failed and offer Terminal; M0 must pin observable completion before enabling the feature. Freeze the completed proposal in bridge-owned state for that generation and hash; later agent edits cannot replace the reviewed bytes.
5. `memory-proposal` for the current ready generation returns the diff from that generation's **original complete base** to its frozen proposal, plus generation, workspace revision, base hash, base-existence flag and proposal hash. A missing file differs from an empty file. Recheck the live memory against the base; a changed live file is `conflict`, requiring a new close-out. Never silently diff a stale proposal against a new base. Identical valid content returns `empty: true` and can be acknowledged without writing memory.
6. Apply takes Task id, generation, the displayed workspace revision and both hashes. Lock the canonical workspace memory path across all daemon and fallback writers; under that lock validate generation/state, run binding, current live base (including existence) and frozen proposal. A mismatch is `conflict`. Back up the full prior file (if any) in `<root>/.muxa/memory/<generation>-<task>.md`, keep newest 20, then temp-write, flush and atomically replace the live file. Mark applied durably. Define retry idempotency so a crash after rename can recognize the exact already-applied proposal and complete the record without another replacement or duplicate backup. A retry after a newer workspace update must conflict. All memory mutation paths acquire the Task record lock first, then the workspace lock, and release them in reverse order; no path may acquire a Task lock while already holding a workspace lock. Hold both locks through final generation/run validation, replacement and the applied record update; restart and discard take the same Task lock and cannot invalidate the generation during apply. Test that lock order across apply, restart, discard and retry.
7. Discard takes the generation and marks it declined. Old generation output is unavailable to later requests, even if kept for inspection. Retry allocates a new generation and a new base. Clients never apply or discard a generation they did not display. Applying does not trigger another prompt, a loop or Task status change. After Apply/Discard/Skip, the client may separately request Done.

**File handling.** Base and proposal share the 64 KiB cap, UTF-8/no-NUL and regular-file checks. A missing base is permitted; missing proposal is not ready. Refuse symlinks at the final files and at `.muxa`, proposal-generation and backup parents. Anchor operations to a validated canonical workspace root/Task directory and use descriptor-relative, no-follow creation/open/rename checks rather than check-then-open path assumptions. Validate containment after canonicalizing roots and when directories are replaced. Test symlinked parents and swap races. Bound diff output to 2 MiB. Never log memory, diff, instructions or secret values.

**Crash recovery journal.** Under the workspace lock, maintain a durable workspace revision and an apply journal containing operation id (Task id plus generation), Task/run, expected revision, base existence/hash, proposal hash and frozen bytes reference, and prepared/committed state. Flush prepared state before rename; flush the file/directory and committed revision after rename. Every apply, snapshot capture and revision-bearing proposal read must recover a prepared operation before trusting or exposing a workspace revision. If recovery requires another Task lock, return recoverable busy/conflict and perform a separate ordered recovery pass; never expose renamed bytes with the pre-crash revision. If recovery observes the expected old base, it can finish the prepared write; if it observes the prepared proposal with no intervening committed operation, it can finish that operation's commit. Any unexplained content becomes conflict. Retries require the operation id and revision, not merely matching bytes. A later muxa generation writing identical bytes, or restoring earlier bytes, is still a newer revision and makes the old retry conflict. Resolve the corresponding Task record without reversing the Task → workspace lock order; journal recovery must not acquire another Task lock while holding the workspace lock. Return a recoverable busy/conflict result if that requires a separate recovery pass. The journal coordinates muxa writers; arbitrary editors ignoring the lock have the limitation below.

**Concurrent writers.** Per-Task record locks do not serialize two Tasks writing one workspace. The canonical-path workspace lock is shared by all muxa writers, including independent fallback bridges; use an OS lock with process-death release and a safe lock-file location. Hold it across compare, backup and replacement. This serializes muxa applies, not arbitrary editors that ignore the lock. Recheck immediately before replacement and document that concurrent external editor writes cannot be guaranteed by a cooperative lock; conflict protection is strongest between muxa writers. No claim of a filesystem-wide transactional compare-and-swap.

The Reader lists the Task snapshot, current generation instructions and validated proposal, and offers the live workspace document via a contained virtual path `workspace/WORKSPACE.md`, `kind: workspace`. Review proposal is enabled only for ready generations. The live file never shares a write endpoint with `doc`.

### W3 / M2. Protocol, UI and CLI

Capabilities `workers` and `workspace-memory` are independent. Advertise `workers` only when create, keep/set/clear restart, observed-run binding and command refresh work end to end; a config listing alone is insufficient. Worker loop defaults require the separate `task-loop` capability to run. Advertise `workspace-memory` only when complete snapshots, close-out, review, apply/discard and recovery work end to end. Additive record fields preserve version 1 unknown-field round trips. Only advertise a capability once its implementation and probes pass. Update PROTOCOL.md, fixtures and Rust/Swift/Kotlin types together before implementation.

- `task-list.workers[]`: name, agent, optional description/workspace scope/loop defaults. Never args, instruction text or env. Task `worker` is optional; each new run has explicit `assignment` and legacy runs may omit it. Release W does not add `managedEnvKeys`. Restart optionally carries the displayed record `rev`, required of worker-aware clients.
- New Task: worker picker filtered by workspace, Workspace default first. Restart: Keep current, Workspace default and allowed workers, with a clear handoff confirmation. Changes to config take effect on the next run. Detail and run history name the worker; Inbox can show it as secondary text.
- CLI: `task new --worker NAME`, `task restart ID --worker NAME`, `task restart ID --workspace-default`; omission keeps assignment. `tasks --json` includes worker definitions and record/run assignments.

| Request | Fields beyond request id | Reply before result |
|---|---|---|
| close-out | task | task-saved with new generation/pending state |
| memory-proposal | task, generation | generation, workspaceRevision, base, baseExists, proposal, diff text, empty |
| apply-memory | task, generation, workspaceRevision, base, baseExists, proposal | task-saved; conflict on stale identity/content |
| decline-memory | task, generation | task-saved |

`closeOut` states are pending, sent, ready, failed, applied and declined; include generation, run, workspaceRevision, since, base/baseExists, optional proposal hash and safe failure reason. Unknown states have no known action. Extend durable pending-prompt identity with generation/run rather than assuming today's fresh-done waiter is sufficient. Requests without the capability are unsupported; absent agent pane is task-failed; incomplete output is a safe not-ready error with no writes. No-session Task close-out does not create an agent.

CLI `task close-out ID` prints the new generation. `task memory ID --generation G` prints the reviewed diff and its identity. Applying requires `--generation G --apply BASE PROPOSAL` and the displayed workspace revision/base-existence values; discard requires the generation. Explicit CLI apply is an intentional write request, not proof that a human initiated it. Apps retain the displayed identity, handle conflicts by offering a new close-out, and never silently rebase/apply. Do not mark Done when Apply failed.

### 5. Scope and verification

The deliverable and acceptance table is in `docs/workers.md`, including the integrated W/Q/S/M product milestones. ADR 0025 separately proposes daily Inbox/manual-session navigation (Q) and scheduled project intake (S); these are not folded into Workers or memory capability gates. The shared identities are defined in `docs/work-model.md`: a Task is durable work, a Session is its optional execution location, a Run records one agent execution, and Inbox references a human need. Ship W independently of M; memory's new-turn detection and apply journal must not block named workers. Both are independent of Inbox slice 1.

Release W includes definitions, native role references, create/restart assignment, instructions, scopes, check defaults, CLI and both apps. Release M adds complete workspace snapshots and reviewed close-out. Per-worker environment/account overrides, workflow graphs, automatic assignment, budgets, cross-host memory, syncing native agent config and automatic apply are out of scope.

## Consequences

Workers reuse native roles and add an explicit assignment/handoff contract. Memory stays a readable file with reviewed, bounded proposals. Keeping release W on existing workspace environments avoids adding an unverified credential layer. Release M has its own durable transaction and observation gates to prevent stale replacement. The human review flow does not claim to sandbox a same-user agent. Existing Tasks without these capabilities keep working.

## Remaining decisions

- W: validate explicit-command respawn, native role startup/discovery, adapter rematching and the restart failure boundary. M: separately validate new-turn completion, coherent snapshots and the memory transaction/recovery protocol.
- Decide whether a workspace-level default worker or separate stable worker id/display name is needed after the first slice. The initial proposal uses Workspace default agent and stable worker names.
- Per-worker environment overrides and exact cleanup remain deferred. They require a new decision before any `worker.env` field is accepted.
- The initial memory cap is 64 KiB for both base and proposal, with rejection instead of truncation. Revisit based on actual memory size rather than silently increasing context.
