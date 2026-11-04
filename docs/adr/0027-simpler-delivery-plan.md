# 0027: Choose the simpler shape for the remaining plan

## Status

Proposed. The developer chose "the simplest shape with the lowest maintenance and change cost" after a plan review. It changes parts of ADR 0022, 0023, 0024 and 0025, listed below. ADR 0033 (proposed) replaces §4's Move down and Later on the Task record; the rest of §4 (manual Sessions, no All Sessions filter) stays.

## Context

- muxa has one developer and one user. No other client uses the protocol today.
- The plan has four tracks: Workers (W), Inbox queue (Q), Routines (S, ADR 0026) and workspace memory (M). Each added contracts, fixtures in three languages and acceptance matrices.
- Task records, records and loops are 12.6k of the bridge's 23.9k lines. Most of it serves restart safety (ADR 0024).
- `workers` is not advertised, so its contract has no user yet.
- The developer uses both Claude Code and Codex. Codex sessions cannot be bound to a process today, so a guarded Codex run gets the fixed prompt in Terminal instead of automatic input (`docs/findings/workers-w0.md`).

## Decision

### 1. Worker selection is a name

Changes ADR 0022's assignment contract. A worker-aware client always sends a worker name. The reserved name `default` is the workspace default agent. A legacy client omits the field and keeps the current worker. The omitted/null/named tri-state is removed. `rev` is optional: a conflict is checked only when it is sent. Run assignment stores the name, or nothing for legacy runs. The W1 types and fixtures change once, before `workers` is advertised.

### 2. Workers ships with Codex continuation by hand

Codex is supported as a worker. After a managed restart, a Codex run gets the fixed prompt shown in Terminal, as today. Automatic Codex continuation needs a session-ownership design (a new ADR). It is required before a Codex agent runs a Routine (ADR 0026), not before `workers` ships.

### 3. The restart acceptance matrix stops at its current coverage

Changes the release gate in ADR 0024 and `docs/workers.md`. The implemented cases stay. The cases not built are listed as known gaps, not as gates: kill points after replacement during publication or rename, and the R01–R16 combinations not yet covered. A failed restart stays visible in the pane, and recovery stays explicit.

### 4. Inbox order is two optional fields on the Task record

Replaces ADR 0025's Q0 and Q2 for Tasks. Move down and Later are optional fields on the Task record (`inbox: {order, until}`), written through `update-task`. The entry id is the Task id. They replace the device-only Snooze of ADR 0023. The only stored Inbox entry is the Routine output (ADR 0026). The manual Sessions list of ADR 0025 Q1 stays, as a UI change.

- No All Sessions filter. Sessions lists manual sessions only. Task sessions open from Inbox, All Tasks and a Task's Open Terminal. The switcher searches every session. On a phone, More has Sessions again, because the list no longer holds every session (changes ADR 0012).

### 5. Smaller surfaces

- New features share one capability with a level, instead of one capability each. Existing capabilities stay.
- A new feature can ship in the CLI and one app first. The other app follows.
- Cut: the tmux sidebar metadata of ADR 0023 slice 2 (it needs an exception to invariant 2), and input leases for two typing devices.
- Deferred with no date: inline approval cards in Triage (Chat already answers them), and the APNs relay (it needs a server).
- Deferred: the Linux daemon (systemd), and tmux-agents as a desktop client (ADR 0017, its branch stays unmerged).

### 6. Workspace memory becomes a Routine

Replaces release M of ADR 0022 (M0–M5, the close-out lifecycle and `workspace-memory`): a Routine (ADR 0026) proposes a WORKSPACE.md change as an Inbox proposal with a diff. Approval creates a Task that writes the file. Run-now from a Task's detail, before Done, replaces the close-out request. No generation, workspace revision, journal or workspace lock is added. `docs/findings/memory-m0.md` stays as background; its M01–M17 cases are not built.

## Consequences

- Workers, Inbox order and Routines reuse the record, the lock and the Loop that exist.
- Less protection: no conflict check when a client omits `rev`; no tested recovery for the listed kill points.
- Lost for memory: recovery between rename steps and ABA protection on WORKSPACE.md. The file is small Markdown, usually in git.
- Codex users continue a restarted run by hand until the ownership design ships.
- One app can lag behind the other for a while.
