# 0046: Delete old Inbox items; protect the ones to keep

## Status

Proposed. It adds to ADR 0033 (the Inbox of questions) and ADR 0040 (Later moves an item down).

## Context

- The Inbox grows. Nothing removes an item that nobody answers: every finished turn makes a `review`, and every Routine run can publish briefings. The host also lists the newest 100 closed items.
- ✓ and Later act on one item at a time. The developer wants to clear old items in one step, and to keep a few of them however old they are.
- Each item has a `key`: the need that made it (`review:<at>`, a Routine's own key). The producer and a Routine's publish skip a key that is stored. When the item is gone, the same need would make it again on the next look.

## Decision

1. **`delete-inbox {olderThanDays}` removes old items.** Every item whose `createdAt` is more than that many days ago goes: open or closed, a Task's item or a Routine's entry. The apps and muxa for tmux offer 1, 3 and 5 days. The host takes 1 to 3650.
2. **Delete removes; it does not answer.** Nothing reaches the agent or the Task. A Task stays open, and its agent may still wait.
3. **The host remembers the key of a removed item**, per Task file and per Routine state (`deleted`, oldest first, at most 50 per Task and 200 per Routine, like the items). The same need does not make the item again. A new need makes a new item in its place.
4. **`protect` marks an item to keep**: `protectedAt`, cleared with `unprotect`. A protected item still takes ✓, Later and every other answer. Only delete skips it. Like `read`, the two actions are accepted on any item, open or closed, and are never in `answers`.
5. A proposal whose approval is creating its Task is kept, as pruning keeps it.
6. **`work` level 8.** An app shows Protect and Delete only on a host with level 8.
7. The apps and muxa for tmux confirm a delete with a count: the listed items it removes. The list carries at most 100 closed items, so the host may remove a few more closed ones. Protect needs no confirmation.

## Consequences

- One step clears the Inbox, and the protected items stay in it.
- A deleted `ask` does not come back while its agent waits on the same menu. The agent's session still shows the wait.
- Alternatives not taken:
  - Delete only closed items: most of the list is open items nobody answered.
  - Delete as an answer (✓ on many items): ✓ on a `review` keeps the Task open but is still an answer. A Routine's proposal would need Reject. Removal is not an answer.
  - Never delete open `ask` items: the developer chose one rule for every kind. Protect keeps an `ask` that matters.
  - Protect by kind: the developer chose a manual mark only.
  - Expire items automatically after N days: the developer asked for a step they take.
