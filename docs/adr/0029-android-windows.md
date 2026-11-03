# 0029: One connection per Android window

## Status

Proposed. It amends ADR 0013's multi-window bullet, which planned one connection for every window.

## Context

- Android runs two windows of one app side by side: split screen, desktop windows, and the system's "new window" action (`PROPERTY_SUPPORTS_MULTI_INSTANCE_SYSTEM_UI`). The user wants a different pane in each.
- A bridge watches one pane per connection (`docs/plan.md`, "One event loop owns the watch"), and its `viewport` sizes that pane's window (ADR 0020, 0021). Two panes over one connection would need a protocol change: a watch per pane, a viewport per watch, and a pane id on every `screen` and `output`.
- Each window has its own size and keyboard, so each needs its own viewport anyway.

## Decision

- **Each window has its own `AppModel`**: its own `PaneSession`, so its own SSH connection, bridge, `viewport` and device session. The saved hosts (`HostStore`) and the device key (`ClientKeyStore`) are the app's, shared by every window. Nothing else is shared.
- The models live in the `Application` (`Windows`), keyed by a window id in the Activity's intent and saved state. A fold, a rotation or a resize keeps the model, as ADR 0013 requires. After a process restart, a window reconnects to its own host and pane from its saved state.
- **The main window** (from the launcher) keeps its model when it closes, as the one window did before. **Another window** that the user closes disconnects at once (`bye`). Going to the background suspends every window after the grace period.
- **Entry points:** "Open in New Window" in the pane's More menu opens the shown pane in a new task with `FLAG_ACTIVITY_NEW_TASK | FLAG_ACTIVITY_MULTIPLE_TASK | FLAG_ACTIVITY_LAUNCH_ADJACENT`: beside the window when the screen has room, else in Recents. The system's "new window" action opens the host and pane last used. The user then picks another pane in either window.

### Both windows on one tmux window

Two bridges watching panes of the same tmux window each hold their own device session that links it. tmux treats them as two devices (`scripts/repro/two-device-sessions.sh`):

- The window takes the size of the device client that switched last: the window the user typed in or resized last. The other window shows its pane scaled to fit, as ADR 0020's "two devices on one window" limit says.
- The bridges never fight: neither takes the window back except on typing or a new viewport (ADR 0020, steps 4 and 5).
- When one window closes, the window takes the other window's size at once. When both close, a desktop client's size returns.
- Invariant 2 holds: each bridge acts only on its own device session; the user's session, its current window and layout stay.

## Consequences

- Two windows cost two SSH connections and two bridges on the host. Each window lists sessions and gets alerts on its own.
- A window's Disconnect ends only that window's connection.
- Rejected: **one connection for every window** (ADR 0013's plan). It needs several watches per bridge, which is a protocol change, and one bridge would need two viewports for one tmux window. Kept for "watching several panes" in Later, if that needs it.
- Rejected: **a model per Activity in a `ViewModel`**. A `ViewModel` dies with its Activity, so the main window's grace period and the reconnect after a recreation would need a second holder anyway.
