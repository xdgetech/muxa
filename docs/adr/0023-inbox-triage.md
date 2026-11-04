# 0023: Inbox triage and Task visibility

## Status

Accepted for slice 1. Slice 2 remains proposed. UI specification: `docs/inbox.md`. ADR 0033 (proposed) replaces what is left of it in the Inbox: need labels, groups and the local Snooze.

## Context

Session lists show agent states but do not identify the Task, its goal or its loop. The existing Tasks screen groups records but requires several navigation steps to act. The developer approved a first slice for visibility and navigation, followed by a separate slice for actions inside Triage.

## Decision

### Slice 1: visibility and navigation

- On iOS and Android, a session with a matching `taskId` shows the Task title, session name and branch, Inbox group with the reported state's age, and loop badge. Manual sessions stay unchanged. Search includes Task titles. Closed Tasks retain their Closed badge even if their session survives.
- Rename Tasks to Inbox. Retain its five-group List and existing detail. Add Triage: one card with the need and navigation to the existing session, Chat, Terminal or Reader. Approvals and follow-up composition remain in the existing Chat screen. Triage does not acquire a chat subscription or send pane input.
- Preserve the shared Inbox grouping rule. Add the following primary need (first match wins). Secondary loop facts remain visible when an answer or agent error is primary.

| Need | Condition | Group |
|---|---|---|
| `none` | Task done or dropped | Closed |
| `answer` | believable agent waiting | Needs attention |
| `agent-error` | believable agent error | Needs attention |
| `loop` | loop paused, or stopped for a reason other than user/status | Needs attention |
| `none` | agent running | Running |
| `review` | agent idle or done | Ready for review |
| `no-session` | no matching session | No agent |
| `no-agent` | session with no believable agent state | No agent |

- The queue includes needs other than `none`, in group order: Needs attention, Ready for review, No agent. Preserve descending `updatedAt`, then descending Task id within each group. This is record recency, not time waiting. All clients use that tie break.
- Next and Previous change focus only. Keep focus by Task id when records reorder. At the end show a summary with Start over and Show List; do not automatically wrap. An item leaves the underlying queue only when host data gives it no need. Existing detail actions remain available through List/detail.
- Snooze hides an item locally until its attention identity changes, until disconnect, or until the app exits. Scope it to host and connection generation. The identity includes Task id and revision, session id, agent pane id, primary need, believable agent state and since, loop state/reason/since. A newly linked session must reappear even if no adapter state exists. Record edits may unsnooze conservatively. Snooze is not an approval prompt identity: a menu change with unchanged list state is not observable in slice 1. Show snoozed count and a Show snoozed control, and keep all items in List.
- Fetch both Tasks and sessions before deciding initial navigation. On phones, auto-open Triage once per connection only if an unsnoozed Needs attention item exists and the user has not already navigated. Ready for review alone does not redirect. Tablets keep the pane first. Reconnect refreshes the data before deciding; it must not interrupt an explicit navigation already made.
- The sidebar shows up to three queue items, their needs, Triage all, and New Task. Empty queues link to List. Gate navigation by host capabilities and available targets; unavailable documents and panes do not get active links.
- No-session Tasks keep existing Done and Dropped actions in detail. Creating a new session for an existing Task is a separate design.
- `muxa-cli tasks --json` adds `need` as an additive desktop contract field. The bridge wire format does not change. All three implementations decode shared grouping/need fixtures.
- The tmux-agents popup can adopt the same needs through the CLI contract. It opens the real session for menus and never answers them. Work in the separate tmux-agents repository requires its own validation.

### Slice 2: direct actions and desktop summary (proposed)

Before implementing direct approval cards or a composer in Triage:

- Define chat ownership acquisition, cancellation and restoration on dismiss, Reader navigation, reconnect and pane exit. Existing `tasksClosed()` is not sufficient restoration.
- Bind each displayed action to host/connection generation, Task, session, pane, chat request and full prompt/options. Invalidate it on navigation, chat reset, disconnect or prompt removal. Adapter options remain the only source of keys (ADR 0009).
- Decide explicitly between the residual race of Chat's raw keys and a guarded bridge answer request. A client-side `prompt.entry` check cannot guarantee the host still shows that menu. Any new request updates the protocol, fixtures and all client types together.
- Follow-ups preserve Chat's paste then Enter behavior and target the displayed pane. Separate attention identity from in-flight action identity. Show pending until request result; errors restore usable controls. Request success alone does not remove an item; refresh host state.

The tmux sidebar/status summary also belongs to slice 2:

- A quiet `ignore-size,no-output` daemon client may stay attached while any Task is open or in review. It never sends a size.
- Proposed metadata: session `@muxa_task_title` and `@muxa_inbox`; global attention/review counts and freshness timestamp. This requires an explicit narrow metadata exception to AGENTS.md invariant 2 before implementation. No sizing, layout or current-window options are included.
- Reconcile titles, reassigned sessions and removed links every refresh; clear obsolete session metadata. Write changed values, but publish a heartbeat after each successful refresh, including unchanged snapshots. Clear counts after the last active Task closes before releasing the client. Reconnect invalidates caches. Failed refreshes do not renew freshness; consumers treat summaries older than 30 seconds as unknown.
- Quote every tmux value through muxa-tmux; rendering must treat Task titles as text, including tmux format syntax. Document the cache contract and verify layout preservation on isolated servers.

Prompt summaries in list rows and persistent snooze are deferred until experience with slice 1 shows a need. Workers and workspace memory (ADR 0022) are independent.

## Implementation and verification

| Phase | Scope | Verification |
|---|---|---|
| P0 | shared needs, deterministic queue, CLI additive field and fixtures | make check, make test-apple, make test-android; preserve all existing fixture groups |
| P1 | iOS/Android Task rows and search | demo UI tests: Task title/state/loop, manual sessions, Closed |
| P2 | iOS/Android Triage navigation, local snooze, shortcuts and phone routing | UI/session tests: focus, ordering, snooze invalidation, reconnect, capability fallback, Reader return |
| P3 | separate tmux-agents popup adoption | that repository's tests against CLI JSON |
| Later | slice 2 contracts, direct actions and daemon summary | new approval identity/lifecycle tests; make test-tmux and make repro for metadata publication |

P0 is first. P1 and P2 build on it. Keep docs/plan.md current as each phase ships.

Acceptance scenarios include: waiting/error with a stopped loop; hidden or stale agent state; equal record timestamps; session linked after snooze; item becomes running; target session disappears; empty queue; host lacks a capability; Reader closes back to the same card; reconnect after explicit navigation. Slice 2 additionally requires another device answering first, changed prompt during a shortcut, failed requests, ordinary Inbox dismissal restoring Chat, daemon reconnect and closing the last Task.

## Consequences

The first slice makes Tasks identifiable and reduces navigation while preserving the existing approval path. Three client implementations remain pinned to one fixture contract. No tmux options, resident client lifecycle or bridge protocol changes are authorized by slice 1. Inline actions and desktop summaries need the additional contracts above before implementation.
