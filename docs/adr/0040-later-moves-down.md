# 0040: Later moves an item down; ✓ closes only the item

## Status

Proposed. It changes ADR 0033 §4 (Later hides an item until a time) and §4a.2 (the titles and More).

## Context

- The developer works through the Inbox from the top down. Later is for an item that can wait: it should go below the rest, not disappear. A hidden item that comes back at a time is a reminder, which the Inbox does not need.
- A `review` offers Done (the Task is done and its session closes) and Keep open (the item closes, the Task stays). A `briefing` offers Done (the item closes). One word means two results, and Keep open reads like Later.
- With one or two buttons, More holds only Later, or Later and one more answer. The menu costs a tap and hides what the item takes.

## Decision

1. **Later moves the item to the bottom of the open items.** The item stays `open` and gets `laterAt` (epoch seconds). The host lists open items not moved down first (`ask` first, then newest), then the moved-down ones in the order they were moved, then the closed ones. An item stays down until it is answered. A new need is a new item, so it is listed in its place.
2. **`later` takes no time.** The host ignores `until`, so an older app's Later moves the item down too. A stored `later` item or entry from before this change is listed as open and moved down.
3. **✓ closes only the item.** On a `review` it is `keep-open`, on a `failure`, an unreadable `ask` and a `briefing` it is `dismiss`. Its accessible name is "Done".
4. **An answer that changes the Task names the Task.** `accept` is **Complete Task**, `drop` is **Drop Task**.
5. **✓ and Later are icon buttons, never under More.** More holds answers only when two or more would be hidden (a `review`'s Redo and Change Course). One answer is shown as a button.
6. The wire action names stay. Only titles and the order change, so no `work` level is added.

## Consequences

- Nothing in the Inbox is hidden. An item moved down several times is still in the list.
- An older app still shows its time picker. Its Later moves the item down, and an older app hides nothing, because the host no longer sends `state: later`.
- Alternatives not taken:
  - Keeping a time as an option: two meanings for one button again.
  - Moving an item down by one place (Move down, ADR 0027): the developer wants low-priority work out of the way, not reordered step by step.
  - "Close Task" for `accept`: close also describes ✓, and the status set is `done`.
