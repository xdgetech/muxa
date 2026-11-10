# 0026: Routines run an agent on a trigger and publish to Inbox

## Status

Proposed by the developer's product direction. ADR 0036 moves `[[routine]]` into each workspace's `.muxa/routines.toml` and replaces the daily `at` with a `schedule`. It supersedes the Routine part of ADR 0025 ("A project routine collects information on a clock" and slices S0–S3). It also replaces ADR 0025's Q0 entry contract: the stored Inbox entry is defined once, with the Routine output (section 4). Nothing in this ADR is implemented.

## Context

- ADR 0025 described a Routine as scheduled, read-only information intake, with one Slack collector as the first path. Slack was only an example.
- Each workspace or project has its own sources: Slack, Linear, Notion and others. Native agents already reach them through their own tools (for example MCP servers). muxa does not need to know each source.
- The developer wants a Routine to do work in a loop and put what needs a decision into Inbox. Writing to an external system is allowed. The main path is an approval in Inbox first.
- The work model (`docs/work-model.md`): a Session is where work runs, a Task is work that runs or ran, and an Inbox entry is a decision for the user. Judging a Task need continues the same Task. Approving a proposal starts new work.
- Runs, restart and delivery claims, observation and recovery all belong to the record in `records/` (`records/mod.rs`, ADR 0018, 0024). The Loop's decision step is independent of the record (`loops/step.rs`); its engine walks records (`loops/mod.rs`).
- Today Inbox entries are derived from Task and session state; nothing is stored (ADR 0023). Only Routine output needs a stored entry.

## Decision

### 1. A Routine is configuration next to workers

A Routine is a `[[routine]]` table in `workspaces.toml`, next to `[[worker]]` (ADR 0022). Its name is its id within the workspace. It has:

- a trigger: a local time with an IANA timezone, or run-now from a client. Event triggers come later.
- an optional worker. Without one, the workspace default agent runs it.
- instructions: what to read, what to do, and what to propose;
- an output policy: item limit, and whether direct writes are allowed (section 5);
- a project label, stamped on Tasks it creates. There is no separate project id; Project stays a label (`docs/vision.md`).
- an enabled flag. Enabling the Routine authorizes its runs. Nothing runs before that.

muxa has no source-specific code. A source is a native tool of the agent. Its login, scope and permissions are checked with the native agent before the Routine is enabled.

### 2. The daemon starts runs on the trigger

The daemon (ADR 0019) checks Routines on its tick: when the next due time has passed and the Routine has no open run, it starts one. Each start has an occurrence id.

- One run per Routine at a time.
- After sleep or a restart, at most one catch-up run, not one per missed time.
- A repeated wall-clock time (DST) is one occurrence. A nonexistent local time moves to the next valid time.
- Expose last success, next due time and the last failure.

### 3. A Routine run is a record of kind `routine-run`

A run is stored as a record with `kind: routine-run`, its Routine and its occurrence id. It reuses everything a Task record has: runs, restart and delivery claims, observation, recovery (ADR 0024) and the Loop engine (ADR 0019). No new store or ownership model is added.

"A Routine run is not a Task" is a statement about the product. Task lists and the derived Inbox skip this kind. The Routine's detail shows its running and past runs, and opens their sessions.

The Loop's check for a Routine run is muxa's validation of the run's output (section 4). A failed check types a fixed prompt, as in a Task Loop. An agent waiting on the user shows a need in Inbox for that run. When the start prompt is never typed, the check stops with `not-sent` (ADR 0019) and the run fails: no fail prompt reaches an agent that never got its task.

### 4. Output goes through a staged, validated publish

The run writes its output to a private stage:

- briefings: a short summary with source links;
- proposals: a concrete action to approve (section 5);
- state: an opaque value for the next run, for example a cursor. The next run receives the last committed state.

Each item carries a `key` chosen by the agent. The host validates the stage: kinds, item limit, sizes and links. It then stores the Inbox entries and commits the state in one step. An item whose `(routine, key)` is already stored is skipped, so a retry or a repeated finding does not duplicate it. The state advances only after a successful publish. Uncertain delivery after a crash is not retried automatically; recovery is explicit, as in ADR 0024.

This stored entry is the only new Inbox entry type. Task needs stay derived from Task state.

Source text is untrusted input. It is never an instruction to muxa and never an approval. Logs keep ids, links, sizes and outcomes only, never message bodies, summaries, prompts or credentials.

### 5. Writes are approved in Inbox and run as a new Task

The main path for an external write:

1. The Routine run publishes a proposal: the action, its target and its content.
2. The user approves, dismisses or defers it in Inbox.
3. Approval creates one new Task through `create-task`, keyed by the proposal. Approving twice still creates one Task.
4. The Task's instructions hold the approved content unchanged, with its hash. The Task's run performs the write.
5. A need of that Task (approval, review) comes back to Inbox as the same Task.

The approval is the only authority for the write. muxa cannot force an agent to do exactly the approved action. The agent that performs writes should have only the native write tools that the action needs. This limit is stated, not hidden.

A Routine can write directly, without a proposal, only when its output policy explicitly allows it. The default is approval first.

## Consequences

- Adding a new source is configuration of an agent and its native tools, not muxa code.
- One record type, one Loop engine and one recovery model serve Tasks and Routine runs.
- Routine runs do not fill Tasks or Sessions. The user sees only decisions in Inbox and the Tasks they approve.
- A Routine run needs automatic prompt delivery. It depends on the Workers release for named workers. A Codex agent can run a Routine only after Codex session ownership is solved (`docs/findings/workers-w0.md`), because a Routine has no person to type the fallback prompt. ADR 0028 provides it behind `[codex] owned_threads`.
- An agent can deviate from an approved proposal. Native tool permissions limit the damage; muxa does not verify the external result.
- Direct writes skip the user's review. They are opt-in per Routine.
- Renaming a Routine in `workspaces.toml` starts a new history. That is accepted for configuration a single developer edits.

## Implementation notes (S0–S2)

Decisions made while building S0–S2, each the simplest that met the sections above:

- **Capability.** The host advertises `work:1`; later features raise the level (ADR 0027 section 5).
- **Session and directory.** A run gets a new directory `<root>/routines/<name>-<id>` in its workspace and a session tagged `@muxa_task` with the run's id. The alternative, one long-lived session per Routine, needs a restart path for each run.
- **Stage.** `.muxa/routine/output.json` in the run's directory, version 1; the committed state is handed in as `.muxa/routine/state.json`. Schema in `protocol/PROTOCOL.md`.
- **Store.** One state file per Routine under the state directory, `routines/r-<hash>.json`, holds the schedule state, the open run, the committed state and the stored entries. One locked atomic write is the publish step. It keeps 200 entries per Routine, closed ones dropped first, so dedup covers those 200.
- **Schedule.** A first look records the latest past occurrence without running it, so a new or re-enabled Routine runs no catch-up. A disabled Routine's last occurrence moves on too.
- **Approval.** The Task's goal ends with `muxa-proposal: <entry> <hash>`; approval finds an existing Task by that line before it creates one. The Task runs on the workspace default agent.
- **Run-now and the bridge.** Runs need the daemon: without it, `run-routine` is `unsupported`. Lists and Inbox actions work in the bridge too.

## Delivery slices

| Slice | Scope | Acceptance |
|---|---|---|
| S0 | `[[routine]]` configuration | trigger, timezone, worker, instructions and output-policy validation; nothing runs before enabled |
| S1 | trigger on the daemon tick, and run-now | no overlap; one catch-up; DST; visible last failure |
| S2 | `routine-run` records and output publish | Task lists and derived Inbox skip the kind; Loop check validates output; stored entries and state commit in one step; `(routine, key)` dedup; explicit recovery; a fake agent first, then one real source |
| S3 | Inbox list and clients | stored entries in Inbox with approve/dismiss/Later; approval → one Task with the content fixed; Routine detail with runs and run-now; CLI first, then one app, then the other |
