# 0091: Later moves an Inbox item to the bottom again

Status: accepted (2026-11-24). It changes ADR 0066 §7's Later and brings back ADR 0040 §1–2 inside ADR 0066's manual order.

## Context

- ADR 0066 §7 made Later hide an item until a chosen time; the item then came back on top. ADR 0040 had moved an item down and hidden nothing.
- The developer marked an item Later and it disappeared. They expected it at the bottom: "it disappeared instead of going to bottom".
- The developer works the Inbox from the top down. Later means "below the rest", not "remind me at a time". ADR 0040 said the same.

## Decision

1. **Later moves an open item to the bottom of the open items.** Nothing is hidden. New items and Reopen go on top, so items moved down stay below them, in the order they were moved. The user can still drag an item anywhere. The item stays there until it is answered.
2. **Later takes no time.** `inbox-later` keeps its name. Its `until` is optional and the server ignores it, so an older app's Later moves the item down too. Apps no longer offer a time picker.
3. **`later_until` leaves the wire.** The server never sends it. An older app hides an item only by `later_until`, so it hides nothing.
4. **Items an older server hid** move to the bottom when the daemon loads `<state dir>/inbox.json`, in their order, and lose `later_until`. Nothing else in the file changes.
5. The server has no Later timer any more.

## Consequences

- Every open item is in the list, in the user's order. The Inbox and Bot counts include items moved down.
- The "Show Later Items" filter and the Later icon on a row are gone. A swipe and `⌘L` / `Ctrl+L` move the item down at once.
- A new app against an older server sends `inbox-later` without `until`. That server refuses it. The daemon and the apps ship together, so this is accepted.

## Alternatives not taken

- **Keep a time on Later** (ADR 0066): one button with two meanings, and the item the developer just saw is gone.
- **A `later_at` mark, with the server sorting marked items last** (ADR 0040 §1): it fights the manual order. A drag would need to clear the mark. Moving the item in the list gives the same order without a second rule.
