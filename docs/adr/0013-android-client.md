# 0013: The Android client, for phones, tablets and foldables

## Status

Proposed. Nothing is built yet. Applies ADR 0012 (layout by size class) to Android.

## Context

- The Android client is the next client after iPhone. It must keep every invariant: it never resizes a pane, it shows the grid at its real cols x rows, and it never logs pane bytes.
- Android runs on phones, tablets and foldables: book-style ones (Galaxy Z Fold, Pixel Fold) and flip ones. By default, a fold, a rotation or a window resize recreates the Activity.
- Android tablets are the closest match to the iPad, the first client. They also run apps in split screen and, from Android 16 QPR1, in freely resizable desktop windows. For apps that target API 36, Android ignores `screenOrientation`, `resizeableActivity` and aspect-ratio limits on displays with a smallest width of 600 dp or more. So the app must work at any window size there. muxa does not use the temporary opt-out (`PROPERTY_COMPAT_ALLOW_RESTRICTED_RESIZABILITY`).
- ADR 0002 deferred a shared Rust client core (UniFFI) "until the Android client starts".

## Decision

### Layout

- **Window size classes** (Jetpack `WindowSizeClass`) choose the layout, as size classes do on iOS:
  - Compact width (< 600 dp) **or** Compact height (a phone in landscape) = the phone layout of ADR 0012: a stack, with the pane pushed over the session list.
  - Medium and Expanded = the tablet layout: list and pane side by side, the list hidden once a pane is open.
- **The layout follows the window, never the screen.** A tablet in split screen or in a small desktop window can be Compact, and gets the phone layout. Resizing the window switches the layout as a fold does, with the same continuity.
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

### Tablets

- The tablet layout is the iPad one: Material 3 adaptive `ListDetailPaneScaffold` for the session list and the pane, and for the Reader's list and document. Its default directive shows one pane at Medium width, so muxa uses `calculatePaneScaffoldDirectiveWithTwoPanesOnMediumWidth` to keep two panes there.
- **Hardware keyboard and pointer.** Tablets are often used with a keyboard case and a trackpad.
  - The key bar shows only when no hardware keyboard is attached, as on iOS with `GCKeyboard`. The signal is `hardKeyboardHidden == HARDKEYBOARDHIDDEN_NO`, not `Configuration.keyboard`, which reports `KEYBOARD_NOKEYS` for some keyboards. An `InputManager.InputDeviceListener` covers hot-plug.
  - The iPad shortcuts map to Ctrl, and are listed in the system shortcut helper (`onProvideKeyboardShortcuts`).
  - Hardware keys reach the IME before `onKeyDown`. The terminal view takes them in `View.onKeyPreIme` only for keys the IME does not use: Esc, Tab, arrows, Ctrl and Alt chords.
  - A mouse wheel or a two-finger trackpad scroll moves the readable viewport; it never sends input to the pane.
- **Multi-window.** Two muxa windows can show two panes side by side (split screen, or desktop windows). Each window watches its own pane over the one connection. This is a later step; the first release has one window. It needs `FLAG_ACTIVITY_MULTIPLE_TASK`, and `PROPERTY_SUPPORTS_MULTI_INSTANCE_SYSTEM_UI` (API 35) for the system's "new window" action. The connection is app-scoped (below), not in a `ViewModel`, which belongs to one Activity.
- The manifest's `<layout android:minWidth android:minHeight>` bounds a desktop window, so the pane keeps a few rows at the 9 sp floor.
- ChromeOS runs the same build. It is not a target, but the same rules cover it.

### Continuity through a fold or a resize

- The Activity declares `android:configChanges="screenSize|smallestScreenSize|screenLayout|orientation|density|keyboard|keyboardHidden"`. A fold, a rotation, a window resize or a keyboard attach then reaches Compose as a new configuration, without recreating the Activity. A desktop drag-resize sends many of them.
- The connection and the session state live in an app-scoped holder or a service, never in the Activity. If the Activity is still recreated (a locale change, or the process restarting), it reattaches to the same session and asks the bridge for the screen again (`watch`).
- The terminal view keeps the keyboard through a fold or a resize. If a recreation happens anyway and the old view had focus, the new one takes it. The pane's grid never follows the window; only the fit (`PaneFit`) does.
- `ProcessLifecycleOwner`'s `onStop` starts a grace period; after it the client sends `bye`, as `suspend()` does on iOS. It ignores Activity recreations, so a fold or a resize never starts it. A minimized desktop window with no other muxa window visible does.

### Client stack (to confirm when the work starts)

- Kotlin and Jetpack Compose.
- Terminal: a fixed-grid view. It must never derive cols x rows from its size: the same problem ADR 0004 solved for SwiftTerm. The candidate is Termux `terminal-emulator` (the emulator only). The Termux app is GPLv3, but its `terminal-emulator` and `terminal-view` modules are Apache 2.0 by the README's exception; confirm in their own LICENSE files. The view is drawn by muxa, at the pane's grid.
- SSH: sshj or Apache MINA SSHD, with host keys pinned on first use and the restricted key of `SECURITY.md`.
- The protocol is tested against `protocol/fixtures`, as Rust and Swift are.

### Shared core

Share only the session core through UniFFI: protocol types, snapshot and stream handling, resync, the chat log. Do not share SSH or rendering; each platform has native libraries for them. This makes the ADR 0002 deferral concrete. Decide it in the first Android task with a spike: if the UniFFI build costs more than the shared logic saves, keep two implementations and the shared fixtures.

## Consequences

- One layout rule for every kind of device: phone, tablet, foldable, and a resizable window. A reviewer checks a new screen against ADR 0012's table, not per device.
- Android needs its own CI job: Gradle, and UI tests on emulator profiles for a phone, a tablet (Pixel Tablet, with a hardware keyboard), a foldable for the hinge states, and the resizable emulator for window resizes.
- Rejected: a WebView or cross-platform UI (Flutter, React Native). The terminal view, IME and hardware keys need native input, as on iOS (ADR 0004).
