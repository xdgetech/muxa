# 0013: The Android client, for phones and foldables

## Status

Proposed. Nothing is built yet. Applies ADR 0012 (layout by size class) to Android.

## Context

- The Android client is the next client after iPhone. It must keep every invariant: it never resizes a pane, it shows the grid at its real cols x rows, and it never logs pane bytes.
- Android runs on phones, tablets and foldables: book-style ones (Galaxy Z Fold, Pixel Fold) and flip ones. A fold or a window resize can recreate the Activity.
- ADR 0002 deferred a shared Rust client core (UniFFI) "until the Android client starts".

## Decision

### Layout

- **Window size classes** (Jetpack `WindowSizeClass`) choose the layout, as size classes do on iOS:
  - Compact width (< 600 dp) **or** Compact height (a phone in landscape) = the phone layout of ADR 0012: a stack, with the pane pushed over the session list.
  - Medium and Expanded = the tablet layout: list and pane side by side, the list hidden once a pane is open.
- **The terminal fit follows the font** (`PaneFit`), with the same 9 sp floor, `.readable` scrolling on both axes and following the cursor, and a pinch between `.whole` and `.readable`.

### Foldables

Jetpack WindowManager's `FoldingFeature` gives the hinge:

| State | Layout |
|---|---|
| Flat, or no hinge | the whole window by size class. A flat book foldable is Medium or Expanded, the tablet layout. |
| Half-open book (`orientation == VERTICAL`) where `isSeparating` or `occlusionType == FULL` | the list on one side of the hinge, the pane on the other |
| Tabletop (`orientation == HORIZONTAL`, half-open) | the pane above the hinge; the key bar and the keyboard below |

- The grid avoids the hinge only when the hinge separates or occludes. A flat fold uses the whole inner screen.
- iOS 27.1 reports the iPhone Duo's hinge (`UIHingeInteraction`: closed, partially open, fully open). The iOS app does not use it yet. If it does, it uses this same table.

### Continuity through a fold

- The connection and the session state live in a `ViewModel` scoped to the navigation graph, or in a service, never in the Activity. An Activity recreated by a fold reattaches to the same session and asks the bridge for the screen again (`watch`).
- The terminal view keeps the keyboard through a recreation: if the old view had focus, the new one takes it.
- `onStop` starts a grace period; after it the client sends `bye`, as `suspend()` does on iOS. A fold does not stop the Activity, so the connection survives it.

### Client stack (to confirm when the work starts)

- Kotlin and Jetpack Compose.
- Terminal: a fixed-grid view. It must never derive cols x rows from its size: the same problem ADR 0004 solved for SwiftTerm. The candidate is Termux `terminal-emulator` (the emulator only). The Termux app is GPLv3, but its `terminal-emulator` and `terminal-view` modules are Apache 2.0 by the README's exception; confirm in their own LICENSE files. The view is drawn by muxa, at the pane's grid.
- SSH: sshj or Apache MINA SSHD, with host keys pinned on first use and the restricted key of `SECURITY.md`.
- The protocol is tested against `protocol/fixtures`, as Rust and Swift are.

### Shared core

Share only the session core through UniFFI: protocol types, snapshot and stream handling, resync, the chat log. Do not share SSH or rendering; each platform has native libraries for them. This makes the ADR 0002 deferral concrete. Decide it in the first Android task with a spike: if the UniFFI build costs more than the shared logic saves, keep two implementations and the shared fixtures.

## Consequences

- One layout rule for four kinds of device. A reviewer checks a new screen against ADR 0012's table, not per device.
- Android needs its own CI job (Gradle, an emulator for UI tests, and a foldable emulator profile for the hinge states).
- Rejected: a WebView or cross-platform UI (Flutter, React Native). The terminal view, IME and hardware keys need native input, as on iOS (ADR 0004).
