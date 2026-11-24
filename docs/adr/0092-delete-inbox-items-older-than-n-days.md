# 0092: Delete Inbox items older than N days

## Status

Proposed (2026-11-24). Brings ADR 0046 decision 1 to protocol v2 and the Inbox of ADR 0066. Wire: `protocol/PROTOCOL-v2.md`, `inbox-delete-older`.

## Context

ADR 0046 let the user delete every Inbox item older than 1, 3 or 5 days in one step. It went away with protocol v1. The v2 Inbox deletes one item at a time. With no daily limit on Bots (ADR 0091), a busy Bot can fill the Inbox faster than the user clears it item by item.

## Decision

1. **`inbox-delete-older {days}`** removes every open and done item whose `created` is more than `days` × 24 hours ago. The host takes 1 to 3650 days. It answers with the Inbox, as `inbox-delete` does.
2. **It deletes as `inbox-delete` does.** It answers nothing and reaches no agent. A removed open Bot item counts as deleted for its Bot. A `once` key stays known, so a Routine's item does not come back. An open need makes a new item when it is needed again.
3. **The apps offer 1, 3 and 5 days** and confirm with a count: the listed items it removes. The host keeps more done items than it lists, so it may remove a few more done ones. The CLI takes any number of days.
4. **Not now:**
   - Protect (ADR 0046 decision 4). It can come back if one day removes items the user wants to keep.
   - Expiry after N days without a step: the developer asked for a step they take, as in ADR 0046.

## Consequences

- One step clears what Bots and Routines piled up.
- Later items count by when they were made, not when they come back, so a step can remove them too.
- An older host cannot parse the new type. It answers `bad-request` without the request's `id`, so the app's request gets no answer. This holds for every request added to v2: there are no capabilities, so the host and the apps are updated together.
