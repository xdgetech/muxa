# 0025: An actionable Inbox queue and scheduled project intake

## Status

Proposed following the developer's product direction. No queue schema, scheduler, Slack connection or scheduled job is implemented by this ADR. It proposes changes to ADR 0023's primary UI and local-only deferral; the accepted slice 1 remains the current behavior until a shared contract is implemented.

## Context

The developer wants an Inbox list processed from the top: act now, finish, or move an item down for later. They also want project routines such as an 08:00 Slack digest that contributes several actionable items. The existing Task Loop checks one Task after an agent turn; it does not poll external sources on a clock. Project is currently a free-text Task field, and Task creation normally launches an agent. Creating agent sessions for every collected suggestion would conflate intake with execution.

## Proposed decision

The shared domain definitions are in [Inbox, Tasks and Sessions](../work-model.md). In particular, a Session can host managed work; the manual-session destination is a filter, not the Session entity definition. Final navigation naming (Sessions/Manual/Terminal) remains open. Integrated milestones are in [the delivery plan](../workers.md#product-milestones-across-the-vision).

### Separate Inbox work from manual sessions

Provide distinct destinations for Inbox, Tasks and Sessions. Inbox is the actionable queue of ToDos, briefings, proposals and Task requests for human input. Tasks holds accepted work, including running Tasks and history; it can remain an All Tasks destination rather than a separate top-level tab. Sessions opens the user's existing manual tmux sessions, with the pane-first experience preserved.

Default Sessions to sessions with no explicit Task or Routine association; an All Sessions filter can expose every session for inspection. Task/Routine sessions remain reachable from their detail/history via Open Terminal. The actual tab/sidebar placement follows size class and is not frozen by this decision.

Classification follows host-owned association, not whether a person or an AI currently types. A manually started Claude/Codex session stays in Sessions until explicitly associated with managed work. A person typing into a Task's Terminal does not turn it into a manual session or remove its Task identity. An unassociated manual session waiting for input can show a badge in Sessions, but does not automatically become an Inbox ToDo. No heuristic creates Tasks or reclassifies sessions from process names or terminal contents.

Starting accepted work moves its execution into Tasks; when the host reports a need for human input or review, Inbox references that same Task. Do not create a second Task for each return to Inbox. Routine collectors belong to Routine run history; their summaries/proposals enter Inbox, rather than their raw terminal sessions. Explicit association and any future conversion of manual work into a Task require their own validated host operation; that conversion is not already implemented.

### List first, with explicit disposition

The main Inbox is a single ordered actionable list. Selecting its first row opens its need and actions; a focus/card presentation is optional. Keep a separate All Tasks view with the existing groups and detail, including running/closed Tasks. This supersedes the primary Triage/List presentation only after implementation.

Offer these distinct actions:

- Act: open the appropriate detail, start accepted work, or answer through the established adapter-owned approval contract.
- Done: explicitly perform the applicable host action. A Task is Done only after host confirmation; a read-only briefing can instead be acknowledged. Viewing or Next never completes work.
- Move down: move the item to the tail of the shared actionable queue without changing Task status or hiding it.
- Later: hide until a chosen host-recorded time, then return it to the queue. This is distinct from moving down and replaces device-only Snooze for the new contract.

The host owns queue membership, ordering, dispositions and revisions across clients and reconnects. Keep focus by item id. Needs and urgency remain visible; the main list does not silently reapply group sorting over an explicit Move down. A genuinely changed need can invalidate deferral and surface new attention, using a separately specified host attention identity. Adapter state remains adapter-supplied. Existing local Snooze remains until this shared replacement is implemented.

Inbox entries can refer to an existing Task, a briefing, or a proposed action. A proposed action is not yet an executing Task. It carries source/provenance and can be accepted into a Task, deferred or dismissed. Existing Task cards remain supported; manual tmux sessions remain unchanged. Accepting the same proposal twice must produce at most one Task. Do not silently convert every collected bullet into a running agent session.

### A project routine collects information on a clock

Keep Task Loop (check/retry inside one Task) separate from a project Routine (scheduled information intake). A Routine has a stable id, workspace and stable project reference, timezone-aware schedule, allowed source scope, collector worker, output policy, item limit and enabled state. Stable project identity is a foundation deliverable; scheduling must not key durable state on a mutable display label. Workspace memory and a general workflow graph are not prerequisites for a minimal Routine.

Example: at 08:00 in the configured project timezone, read the permitted Slack channels since the last successful collection, produce one digest with source links, and publish a bounded set of proposed actions into Inbox. The user then accepts, defers, dismisses or processes those items. The collection run is authorized by enabling the Routine; accepting downstream work is a separate action. The first release reads sources and writes muxa intake records only; it does not post Slack messages or automatically execute the proposed Tasks.

The existing host daemon owns scheduling while apps are disconnected. A Routine does not run when the host is asleep/offline; expose last success, next due time and failure. On wake/restart, perform at most one bounded catch-up collection, not one run per missed clock tick. Define timezone/DST behavior explicitly; repeated wall-clock occurrences share one scheduled occurrence identity, and a nonexistent local time follows a documented next-valid-time policy. Prevent overlapping runs for one Routine across daemon/recovery owners; expired time alone is not proof that a prior external operation did not occur.

Use durable occurrence ids and source ids/cursors. Stage collector output, validate it, then publish a batch, Inbox entries and the successful cursor coherently. Retrying a publish must not duplicate the digest or proposals. Advance the source cursor only after successful publication; partial retrieval must be visible and must not mark an incomplete interval fully consumed. Native collection/typing with uncertain delivery does not automatically replay after a crash; reuse explicit recovery principles from ADR 0024. A source permission/network/model error produces one visible Routine failure with retry, not repeated empty or duplicate Tasks.

Source text is untrusted input, not instructions or approval authority. Host configuration selects sources, workspace environment and native tools. Do not log message bodies, summaries, prompts, tokens or credentials. Keep source links/ids, collection interval, run outcome and proposal lineage so a human can judge what to accept. Connector authentication, scope and native collector support must be verified before enabling a Routine.

## Delivery slices

| Slice | Scope | Acceptance |
|---|---|---|
| Q0 | freeze Inbox entry/order/disposition contract | existing Tasks and future proposals share stable ids/revs; shared fixtures preserve source state and unknown entry fallback |
| Q1 | list-first UI, separate manual Sessions and Move down | iOS/Android/CLI agree on order; stale actions conflict; Next has no status effect; reconnect restores order/focus; manual AI waits stay in Sessions; Task Terminal interaction keeps Task identity; All Sessions retains inspection access |
| Q2 | host Later and proposal disposition | cross-device timed return; changed attention identity; idempotent accept creates one Task; dismissal never closes an unrelated Task |
| S0 | stable project ids and Routine configuration | rename-safe references, timezone/source/worker validation, no implicit native launch |
| S1 | durable daemon scheduling | disconnect/offline/DST/catch-up, concurrent owners, bounded retries and visible failures |
| S2 | one read-only source-to-Inbox path | controlled Slack collector, coherent cursor/batch publication, provenance, deduplication and explicit downstream acceptance |
| S3 | clients and release verification | configure/pause/run-now/history; source login checks, crash tests, caps/limits, real-device Inbox flow |

Q can progress with Workers; S follows reliable worker execution and project identity. Neither waits for the full Phase 4 graph or Phase 5 autonomous assignment. All new wire shapes/capabilities require shared Rust/Swift/Kotlin types and fixtures before endpoints are exposed. This ADR authorizes no deployed cron job, daemon installation or external messages.
