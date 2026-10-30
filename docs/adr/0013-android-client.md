# 0013: The Android client, for phones, tablets and foldables

## Status

Accepted. Applies ADR 0012 (layout by size class) to Android. The client is built in `android/`; `docs/plan.md` says how far.

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

- The tablet layout is the iPad one: the session list beside the pane, hidden once a pane is open, and shown again from the bar. It is a plain Compose row, not `ListDetailPaneScaffold`: the scaffold keeps both panes visible at Medium and Expanded width, and the iPad hides the list. The Reader shows its list and its document side by side.
- **Hardware keyboard and pointer.** Tablets are often used with a keyboard case and a trackpad.
  - The key bar shows only when no hardware keyboard is attached, as on iOS with `GCKeyboard`. The signal is `hardKeyboardHidden == HARDKEYBOARDHIDDEN_NO`, not `Configuration.keyboard`, which reports `KEYBOARD_NOKEYS` for some keyboards. An `InputManager.InputDeviceListener` covers hot-plug.
  - The iPad shortcuts use Ctrl+Shift, not Ctrl: plain Ctrl chords (Ctrl+R, Ctrl+K, Ctrl+N, Ctrl+J) belong to the pane's shell, and Ctrl+Shift is the Linux terminal convention. They are listed in the system shortcut helper (`onProvideKeyboardShortcuts`).
  - Hardware keys reach the IME before `onKeyDown`. The terminal view takes them first (`Modifier.onInterceptKeyBeforeSoftKeyboard`, the Compose form of `View.onKeyPreIme`) only for keys the IME does not use: Esc, Tab, arrows, Ctrl and Alt chords.
  - A mouse wheel or a two-finger trackpad scroll moves the readable viewport; it never sends input to the pane.
- **Multi-window.** Two muxa windows can show two panes side by side (split screen, or desktop windows). Each window watches its own pane over the one connection. This is a later step; the first release has one window. It needs `FLAG_ACTIVITY_MULTIPLE_TASK`, and `PROPERTY_SUPPORTS_MULTI_INSTANCE_SYSTEM_UI` (API 35) for the system's "new window" action. The connection is app-scoped (below), not in a `ViewModel`, which belongs to one Activity.
- The manifest's `<layout android:minWidth android:minHeight>` bounds a desktop window, so the pane keeps a few rows at the 9 sp floor.
- ChromeOS runs the same build. It is not a target, but the same rules cover it.

### Continuity through a fold or a resize

- The layout is a pure function of the window size and the hinge (`windowLayout`, unit-tested). The detail is `movableContentOf`: a new layout moves it, as SwiftUI does on iOS, so the pane view keeps its screen.
- Android clears a moved view's focus. The pane view takes it back when it is attached again, and restarts its input connection; keys typed in the moment before the input method reconnects are lost.

- The Activity declares `android:configChanges="screenSize|smallestScreenSize|screenLayout|orientation|density|keyboard|keyboardHidden"`. A fold, a rotation, a window resize or a keyboard attach then reaches Compose as a new configuration, without recreating the Activity. A desktop drag-resize sends many of them.
- The connection and the session state live in an app-scoped holder or a service, never in the Activity. If the Activity is still recreated (a locale change, or the process restarting), it reattaches to the same session and asks the bridge for the screen again (`watch`).
- The terminal view keeps the keyboard through a fold or a resize. If a recreation happens anyway and the old view had focus, the new one takes it. The pane's grid never follows the window; only the fit (`PaneFit`) does.
- `ProcessLifecycleOwner`'s `onStop` starts a grace period; after it the client sends `bye`, as `suspend()` does on iOS. It ignores Activity recreations, so a fold or a resize never starts it. A minimized desktop window with no other muxa window visible does.

### Client stack

- Kotlin and Jetpack Compose. minSdk 28: the minimum for a Keystore key bound to an unlocked device, and for `ImageDecoder` (HEIC).
- Modules: `:protocol`, `:terminal`, `:transport` and `:session` are plain JVM code with JVM tests; `:app` is the Compose UI.
- **Terminal: JetBrains JediTerm core** (`jediterm-core`, Apache 2.0 or LGPLv3), the emulator only, fed bytes on its own thread. muxa draws the grid on a Compose `Canvas` at the pane's cols x rows, and drops the emulator's replies. Its core has no AWT or Swing. It replays `protocol/fixtures/scenarios` to the screens the host recorded.
  - Termux's `terminal-emulator` was rejected: Termux's own changes in it are GPLv3, which would rule out the app's commercial license.
  - JediTerm is published on JetBrains' Maven repository, not Maven Central. Gradle takes only that group from it.
- **SSH: sshj** (Apache 2.0), with host keys pinned on first use (exact match, as on iOS) and the restricted key of `SECURITY.md`. Ed25519 comes from BouncyCastle; Android ships a stripped provider named `BC`, so the app puts the full one in its place at start.
- **The device key** is ed25519, generated in the app. Its private bytes are sealed with AES-GCM under an Android Keystore key bound to an unlocked device, the analog of `WhenUnlockedThisDeviceOnly`. It is excluded from backups.
- **No logs of content.** JediTerm and sshj log through SLF4J, which can carry escape sequences and packet bytes. SLF4J is bound to its no-op provider.
- The protocol is tested against `protocol/fixtures`, as Rust and Swift are.

### Shared core

**Kotlin, not UniFFI.** The Swift client has its own session core, not a Rust one. A UniFFI core would serve only Android, and it would add an NDK build for four ABIs and async callbacks across the FFI. The contract stays `protocol/fixtures`, which Rust, Swift and Kotlin all test against. This closes the ADR 0002 deferral.

## Consequences

- One layout rule for every kind of device: phone, tablet, foldable, and a resizable window. A reviewer checks a new screen against ADR 0012's table, not per device.
- Android needs its own CI job: Gradle, and UI tests on emulator profiles for a phone, a tablet (Pixel Tablet, with a hardware keyboard), a foldable for the hinge states, and the resizable emulator for window resizes.
- Rejected: a WebView or cross-platform UI (Flutter, React Native). The terminal view, IME and hardware keys need native input, as on iOS (ADR 0004).
