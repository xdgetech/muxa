# 0051: Staff and hand-offs inside a workspace (Personal Assistant PA4)

## Status

Proposed on 2026-11-10; its questions were answered with the recommended values, and staff are workers. It extends ADR 0048 §4, §7 and §8 and ADR 0049 §1, §3 and §4 without changing them, and keeps ADR 0049 §4's rule that delegation crosses workspaces one level deep, through the overall assistant and the user. It raises one limit of ADR 0022 (a worker's `description`). It does not change ADR 0050. Slices D0 (the wire types) and D1 (`staff`, `lead`, `handoffs` and `max_steps` in configuration) are built; hosts do not advertise level 12 yet.

## Context

- A workspace has one delegate: `[workspace.assistant].worker`, else the workspace's agent (ADR 0048 §7), with one `NOTES.md` (ADR 0049 §1). Every request, whatever its scope, runs as that one agent with that one memory.
- The user's picture: "each workspace is like a company and they should have their own employees specialized in something and they should work on the work together as a team". Each staff member should feel persistent, and the secretary (the overall assistant) coordinates the companies.
- The host already defines a specialist. A `[[worker]]` (ADR 0022) is a name, a `description`, an agent command with its `args`, an `instructions` file and a `workspaces` allowlist: its own permissions, MCP servers and settings. `lisk-research`, `lisk-delivery` and `lisk-meetings` on the developer's host differ only in what they may read and run. The user's conclusion: worker definitions already exist, so a staff member is a worker. Spec §6 agrees: a 担当 is a persistent responsibility with its settings, not a resident conversation; a Worker is its execution setting.
- A run is one brief, one agent, one Result (ADR 0048 §4). The brief is the goal of `TASK.md`, at most 16 KiB (`records::MAX_GOAL`); a run's earlier Results and Tasks are already in it as "Earlier work". Runs already record their `worker` (ADR 0027). The overall assistant's brief lists each sharing workspace by name (ADR 0049 §3) and proposes `delegate {work, workspace, scope, purpose, reason}` (§4).
- Spec §9: a request has a parent, a destination and a depth; the host limits deliveries, retries and cost per parent and stops loops. Spec §12 names a general workflow graph as a non-goal.

## Decision

### 1. A workspace's staff is a list of workers

`[workspace.assistant] staff` names the workers of a workspace with `delegate = true`:

```toml
[workspace.assistant]
delegate = true
staff = ["lisk-research", "lisk-delivery", "lisk-meetings"]
lead = "lisk-research"   # optional; else the first
handoffs = "review"      # review (default), auto or none
max_steps = 3            # at most 5
```

- Each name is a `[[worker]]` allowed in this workspace (`workspaces`) and a Claude Code command (ADR 0048 §7); a duplicate, an unknown worker, one not allowed here or a Codex worker fails config, not a run. At most 8 names. A worker in `staff` needs a name that is one path segment (ASCII letters, digits, `-`, `_` and `.`, not `.` or `..`), because it names a notes folder (§5); other workers keep ADR 0022's rule.
- **The job description is the worker's `description`.** It is required for a worker in `staff`, at most 1 KiB without control characters; ADR 0022's 200 characters become 1 KiB for every worker, so one rule holds. It goes into the brief as "You are `<worker>`: …" and into the overall assistant's brief as the member's description. It is a description, not a permission: what a member may read or run is its `args` and settings. Its `instructions` file keeps its meaning: read at each start and given to the agent as for any worker's run (ADR 0022).
- The lead takes the requests that name no worker (§2) and keeps the workspace's shared notes (§5). `lead` must be in `staff`; without it, the first name is the lead.
- **A workspace without `staff` has a staff of one, the delegate of today:** `worker`, else the workspace's agent. Its notes, budgets and brief are ADR 0048's and 0049's, unchanged; it is the lead. `staff` and `worker` together is a config error, with the fix in the message: list the worker in `staff` and name it `lead`.
- The overall workspace refuses `staff`, as it refuses `delegate`.
- No per-member budgets, scopes or env. A step is a run and counts against the workspace's and `[assistant]`'s budgets; who answers which scope is the lead's judgment, not a table; ADR 0022 refuses worker env.
- The list lives in host configuration, not in `<root>/.muxa/`, because a worker is a grant of permissions (ADR 0022: args are host-owned). Agents write under root; they must not be able to add a member that runs a wider worker.

### 2. Who picks the worker

- **The user.** `request-assistant` and `capture {assist}` gain `worker?`. A name not in `staff` is `bad-request` with the staff names.
- **The overall assistant.** Its brief lists, for every workspace with `share` other than `none`, the staff's names and descriptions: configuration, not work data, so `outline` is enough. A `delegate` proposal gains `worker?`. Approve passes it to `request-assistant`; a name the brief did not carry is refused by the plan check, as an unknown id is.
- **The lead.** A request with no worker goes to the lead. It may answer it, or hand it on (§3). A request from the overall assistant with no worker goes to the lead too.

### 3. Teamwork is a chain of runs on one request version

A request version keeps `steps[]`: `{step, worker, by, delivery, deliveryAt, run?, handoff?}`. `step` counts from 1; `by` is `user`, `overall`, `lead` or `handoff`. The request's `delivery` is its newest step's. Each step is one `assistant-run` with one brief and one Result, as today.

- **Hand-off.** `result.json` gains `handoff? {worker, purpose}`: "pass this to that member next", `purpose` 1 byte to 1 KiB. One hand-off per Result, so the chain is a line, not a graph: `lisk-research` → `lisk-delivery` → `lisk-meetings`. The check refuses a worker not in `staff`, the run's own worker, a worker already in the chain, and any hand-off from the last step. A hand-off does not change the request's `scope`; its `purpose` says what the next member does within it. The brief states the staff, the step number and these rules, so a refusal is the agent's error, and the fail prompt says which.
- **Approval.** `[workspace.assistant] handoffs` is `review` (default), `auto` or `none`. `review`: the stored Result makes one Inbox item of kind `handoff` in the workspace, question "`<worker>` asks `<next worker>` to …", with `approve`, `reject` and `later`; approve starts the step at the request's current version, reject closes the chain with the Results it has. `auto`: the host starts the next step at publish, within every budget, and makes no item; Work detail shows every step, which is where the user sees the team's work. Past a budget the host makes the `review` item with the limit in its summary and does not try again, as ADR 0049 §4 does for `auto` requests. `none` refuses the field in the check. The default is `review`, because a page or document read by one member can plant a hand-off purpose that the next would then follow.
- **Limits.** `max_steps` is 3 by default, at most 5. A worker appears once per chain. A chain holds one open run; a new request version ends the chain's open run and starts step 1 again; `cancel-request` cancels the open step and closes the chain. Each step is a run and counts against the workspace's and `[assistant]`'s budgets; `[assistant] max_auto_handoffs_per_day` (default 10) caps `auto` steps across the host.
- **Once.** One `(request, version, step)` makes one run, found again after a crash by the goal line `muxa-request: q-… v<n> s<k>`; a line without `s<k>` is step 1. The `creating` claim of ADR 0048 §3 carries the step. A `handoff` item's approve is keyed by item id and proposal hash, as Routine approval is; a second approve returns the step it made.
- **Results.** Each step's Result is stored on the version with `worker` and `step`. A newer version marks every older step's Result `stale`; inside a version nothing is stale, a chain is one answer in parts. The newest step's Result is the version's current one for the overall assistant's `summaries` (ADR 0049 §3), for `possibleDuplicates` and for Work detail's head; the earlier steps show under it.

### 4. Across workspaces: still the overall assistant and the user

A hand-off names a worker of its own `staff` only. Members of one workspace share one root, one env and one set of credentials, so passing a full Result between them discloses nothing new. Another workspace is another company: what crosses is filtered by `share` and asked through a `delegate` the user approves (ADR 0049 §3, §4). A step whose open question needs another workspace writes it in `openQuestions`; the user answers it, or the next plan proposes a `delegate`. Nothing in this ADR adds a path from one workspace's run to another's.

### 5. Continuity: the lead's notes and each member's notes

| File | Who reads it | Who proposes changes |
|---|---|---|
| `<root>/.muxa/assistant/NOTES.md` | every member, as data | the lead only (the delegate of today) |
| `<root>/.muxa/assistant/staff/<worker>/NOTES.md` | that member | that member |

Both follow ADR 0049 §1: copied into the run folder at start with their SHA-256, 16 KiB, regular file, `notes` in the Result is the member's own file's full text, `notes = "review"` or `"auto"` applies. The lead's own file is `NOTES.md`, so a staff of one is unchanged. `apply-notes` gains `step?` (default the version's last step); the host knows the step's worker and so its file. A member's run reads the workspace notes as data and may not propose them; the brief says to put a fact every member needs into `openQuestions` for the lead, or to ask the user. Forgetting stays a file edit.

The brief of a step carries: the member's `description`; the Work item, its Captures and the request's purpose (ADR 0048 §4); the hand-off's `purpose` and the worker that sent it; the earlier steps of this version with their `files`, in full in `.muxa/assistant/steps/<k>.json` in the run folder, and in the goal as each step's worker and summary cut to 600 characters, so the 16 KiB goal keeps room; earlier versions' Results as today; the staff with the step number and the hand-off rules; both notes copies; the result schema. Logs carry worker names, step numbers and ids, never descriptions, purposes or hand-off text.

### 6. Wire: additive, `work:12`, protocol v1

| Request | Reply |
|---|---|
| `request-assistant {…, worker?}`, `capture {assist {…, worker?}}` | as today; a worker not in `staff` is `bad-request` |
| `update-inbox {entry, action}` on a `handoff` item | `inbox-saved`, with `work-saved {work, entry, request}` on approve; a budget's `conflict` leaves the item open |
| `apply-notes {source, version, base, step?}` | as ADR 0049 §7 |

A request gains `steps[]`; a Result `worker?` and `step?`; `inbox-list`'s `assistant.delegates[]` gains `staff[] {worker, description, lead?, notesAt?}`; a `delegate` proposal and its item gain `worker?`; `result.json` gains `handoff?`. A level 11 client shows a request's newest step as the request, ignores `steps`, `staff` and the `handoff` kind, and keeps working. The full contract goes into `protocol/PROTOCOL.md`, "Staff and hand-offs"; fixtures in `protocol/fixtures/assistant/`. Clients send `worker` and the new action only at `work:12`; a level 11 host answers `bad-request`. Branches that also raise the level renumber on merge.

### 7. Clients

- **Words:** configuration keeps `[[worker]]`; clients and docs say "staff member" ("staff" for the list), at the user's request: a worker does not sound like an employee. The wire field stays `worker`.
- **Ask Assistant:** a staff picker when the workspace has more than one member, default "Let `<lead>` decide"; the worker's description as its subtitle.
- **Work detail:** the chain as rows "`lisk-research` → `lisk-delivery` → `lisk-meetings`", each with its delivery, Result and Update notes; a `handoff` waiting for approval shows as the next row, greyed, with Approve and Reject.
- **Inbox:** `handoff` items with Approve, Reject, Later; the title names both workers.
- **Today:** a `delegate` row names the worker when the plan picked one.
- **muxa for tmux:** `a` on a Work row asks for the worker (`Enter` for the lead); the Inbox shows `handoff` rows with `y` / `n`. CLI: `assist WORK --worker NAME`, `assist staff WORKSPACE`, `inbox answer ITEM approve`.

## Not decided

- External writes "only when asked directly": a worker's permissions are static, so a staff member is read-only toward outside systems. An approval step for a Result's proposed external write, like a Routine proposal under `direct_writes = false` (ADR 0026), is decided in ADR 0052.
- Fan-out: a lead that asks two members at once and merges their Results. This ADR: a line of at most `max_steps`; a graph is spec §12's non-goal until a chain proves too narrow.
- A second round: reviewer → writer → reviewer. This ADR: a worker appears once per chain; the reviewer's findings go to the user, who asks again with a new version.
- Whether a Staff screen shows each member's notes and lets the user edit them from a client. This ADR: the file is edited on the host, as `NOTES.md` is.
- Whether a workspace-scoped policy (ADR 0050) enters a member's brief. ADR 0050 says no for the delegate; the same here.
- `max_steps` 3, `max_auto_handoffs_per_day` 10, 8 members and the 600-character summary cut are initial values.

## Consequences

- A workspace reads like a small company: named workers with job descriptions, each with its own tools and memory, passing work along a visible chain the user approves or lets run. Nothing resident: a member is a worker plus a notes file, and every step is one inspectable run.
- No new concept. The host's workers, their allowlists and their descriptions are reused as they are; `staff` only says which of them this workspace's assistant may be.
- No new execution path. A step is an `assistant-run`; hand-offs reuse the Result check, the Inbox item and approve-once, the budgets and the crash recovery of ADR 0048 and 0049.
- The secretary gets better addresses: it can ask `lisk-research` instead of Lisk.
- Costs: a chain is up to `max_steps` runs for one request; budgets bound it, and `review` makes the user approve each step until a workspace earns `auto`. Clients grow a staff picker, chain rows and one item kind.
- Alternatives not taken:
  - A role table beside workers (`[[workspace.assistant.role]]` with its own purpose, worker, scopes and budgets): a second definition of the same specialist, which drifts from the first.
  - One workspace per member: it duplicates root, repos, env and `share`, and members share credentials anyway.
  - Native subagents inside one run (Claude Code's own agents): invisible to muxa, no per-member notes or review of a hand-off. A worker may still use them inside its run; muxa neither relies on nor forbids it.
  - Scope as the member: scopes say what is asked, workers say who answers; `lisk-research` may take both `research` and `compare`.
  - A long-lived team session or chat: ADR 0049 rejected the single-delegate form of it for the same reasons.
  - The list in `<root>/.muxa/assistant/staff.toml`: agent-writable, and a member grants a worker (§1).

## Implementation notes

- `records::assistant::Role` (what a workspace is to the overall assistant) keeps its name; this ADR adds no type called role. `staff` is a list of worker names on the workspace's assistant config, checked against `Workspaces::worker` and `Worker::allows` at load.
- `RunFor::Request` gains `step`; the run's existing `worker` names the member. A stored run without `step` is step 1; one without a named worker is the delegate of today.
- `policy::delegated` resolves `(workspace, worker?)`: the named member, else the lead, through the same `resolve_worker` and `check_agent`.
- The hand-off item lives in the run's item file, keyed `handoff:<request>:<version>:<step>`, as plan proposals are keyed (ADR 0049, Implementation notes).

## Delivery slices

| Slice | Scope | Acceptance |
|---|---|---|
| D0 | this ADR, `PROTOCOL.md`, fixtures, Rust, Swift and Kotlin types | fixtures round-trip in all three; a level 11 client decodes a request with `steps` and a Result with `worker` |
| D1 | `staff`, `lead`, the 1 KiB description, the staff name rule | a duplicate, an unknown worker, one not allowed in the workspace, a Codex worker, a member without `description`, a `lead` not in `staff`, `staff` with `worker`, a name with `/` and `staff` on the overall workspace are refused with the workspace and worker named; a workspace without `staff` resolves as today |
| D2 | `worker` on `request-assistant` and `capture`, resolution, the brief's staff section, member notes | an unknown worker refused; the brief holds the description and the staff; a member's `notes` lands in its own file, never in `NOTES.md`; the lead's lands in `NOTES.md` |
| D3 | `handoff` in the result check, `steps[]`, the step marker, `creating` with step | counterexamples refused: own worker, a worker already in the chain, one not in `staff`, a hand-off from the last step; one `(request, version, step)` makes one run after a crash |
| D4 | `handoff` items, `handoffs = review / auto / none`, `max_steps`, `max_auto_handoffs_per_day` | fake agent on a `-L` server through the daemon: a three-step chain under `review` makes two items and three runs; approve twice makes one step; `auto` runs the chain within budgets and holds the step past one with the limit; `none` refuses; a new version ends the open step; cancel closes the chain; the log grep finds no description, purpose or hand-off text; the layout snapshot is unchanged |
| D5 | the overall brief's staff, `worker?` on `delegate` | synthetic workspaces: staff appear at `outline` and above, never at `none`; a `delegate` naming a worker the brief lacks is refused; approve passes the worker |
| D6 | `apply-notes {step?}`, steps files in the run folder | the right file is written by hash; a changed base is `conflict`; `steps/<k>.json` holds the full earlier Results and the goal stays within 16 KiB |
| D7 | advertise `work:12`; CLI; muxa for tmux | bats; `assist --worker` against a level 11 host fails with "update muxa-bridge" and sends nothing |
| D8 | iOS: staff picker, chain rows, `handoff` items | UI tests on the demo with a three-member workspace |
| D9 | Android | the same |
| D10 | docs: `agents.md`, `examples/workspaces.toml`, `inbox.md`, `work-model.md`, `SECURITY.md`, `plan.md` | — |
| — | **PA4 staff done** | a request to Lisk with no worker runs its lead; the lead hands to `lisk-research`, then `lisk-delivery`; the user approves each step and reads one chain in Work detail; the secretary's next plan names `lisk-research`; no Result or note crosses to another workspace |
