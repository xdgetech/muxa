# 0093: One popup design on the Mac

## Status

Accepted (2026-11-24).

## Context

The Mac app showed popups in two looks. Questions such as Close Task, Remove worktree, the ⌘T chat choice and the remove and restore questions used macOS's own alert: system grey, centred text, the app icon, no keys. Sheets such as New Bot used muxa's look, with a different header and footer. The user asked for one standard design.

## Decision

1. Every popup on the Mac is muxa's card (`DialogCard`, `apple/Muxa/MuxaMac/Design/DialogCard.swift`): the surface colour, a 400 pt width for a question or a choice, a 15 pt semibold title with a lead line, then a footer under a hairline. The footer has key hints on the left, and Cancel (esc) and the action (⏎) on the right. A destructive action is the red button.
2. A choice, such as ⌘T's "start a new chat or continue one", shows rows. ↑↓ pick a row, ⏎ runs it, and ⌘1… pick one directly.
3. Sheets with forms (`SheetScaffold`) use the same header and footer.
4. Code asks through one modifier, `.dialog(...)` (`apple/Muxa/Muxa/Next/Dialog.swift`). On iOS it keeps the system alert, or the action sheet where the app had one.

## Consequences

- A new popup gets the design by using `.dialog`. A plain `.alert` or `.confirmationDialog` on the Mac is a mistake.
- macOS still draws the sheet's frame and the dimming. Only the content is muxa's.
