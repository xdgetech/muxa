# 0020: The window size follows the device in use

## Status

Proposed. It supersedes ADR 0001's sizing decision (`-f ignore-size` and never `refresh-client -C`). Control mode, the snapshot and the stream cut of ADR 0001 stay. It amends ADR 0004: the client still renders the pane's real grid, but it now tells the bridge which grid it would like.

## Context

- muxa promised that the desktop layout never changes (ADR 0001). The tablet showed each pane at the desktop's size, scaled to fit.
- That makes the pane hard to read when the desktop pane is large, and tiny when it is small. A task session made with `new-session -d` is 80x24, and tmux-agents splits it into 25-column panes. The iPad then shows a 25-column pane on a 13-inch screen (`scripts/repro/detached-session-size.sh`).
- The developer decided: unchanged sizes are not what matters. Each device must show the pane in the best way for that device. The chosen option is tmux's own `window-size latest`: while the user works on the iPad or an Android device, the window takes that device's size. Back at the laptop, it takes the laptop's size again. tmux-agents' split follows. The accepted cost: when two devices look at one window at the same moment, only the latest one is optimal.
- The facts, each with a repro script (`docs/findings/tmux-control-mode.md`, "Sizing for the device in use"):
  1. `window-size` defaults to `latest`.
  2. A control client without `ignore-size` that sent `refresh-client -C WxH` sizes the window to WxH while it is the window's latest client. Without a size it sizes nothing.
  3. `attach-session` and `switch-client -t <its own session>` make it latest for the session's current window. Its own `send-keys` and a repeated `refresh-client -C` do not. A desktop key press makes the desktop latest again.
  4. `refresh-client -f ignore-size`, followed by `refresh-client -C`, gives the window back. So does detaching.
  5. It sizes only windows of its own session. With no other client on that session, it sizes all of them, and they keep that size until a desktop client views them.
  6. `smallest` would let it shrink a window that the desktop is using. `largest` and `manual` do not follow it. `resize-window` pins a window to `manual`.
  7. A per-window size (`refresh-client -C @id:WxH`, 3.3+) caps the window in every mode.
  8. The daemon's `ignore-size,no-output` client never sends a size and sizes nothing.
  9. tmux spreads a width change over side-by-side panes. tmux-agents' `window-resized` hook keeps its sidebar width and the work panes' shares. A pane gets a given width only through the window's width.

## Decision

### The rule

A bridge whose client sent `viewport` takes part in window sizing like a desktop terminal under `window-size latest`. The window follows the device in use. A client that never sends `viewport` gets today's behaviour: `ignore-size`, no size, scale to fit.

### The protocol

- Request `viewport {id, cols, rows}`, behind the capability `viewport`.
- `cols` and `rows` are the grid the client's pane area shows at its default font. `cols` is 10 to 1000, `rows` is 5 to 1000. Other values get `bad-request`.
- It holds for the current watch and every later one, until the next `viewport`.
- The client sends it before its first `watch`, and again whenever the pane area changes: a rotation, a split view, a fold, a window resize. It waits 300 ms for the area to settle.
- The client still renders whatever `screen` says. The pane can be smaller or larger than the viewport, for the reasons under "Limits".

### What the bridge does

1. **Attach.** It attaches with `-f ignore-size` and no size, as today. `client-attached` hooks see a control client, and nothing is sized before a watch.
2. **Watch.** After `switch-client` to the pane's session, it reads `#{window-size}` for the pane's window.
   - `latest`: it sends `refresh-client -f '!ignore-size' ; switch-client -E -t <session> ; refresh-client -C WxH`, with WxH from "Pane fit". `switch-client` makes it the latest client; `-E` leaves the session's environment alone.
   - Any other value: it stays `ignore-size`. Under `smallest` the device would shrink the window the user types in on the laptop; under `largest` and `manual` it would not follow the device anyway.
3. **Pane fit.** The bridge chooses WxH so that the watched pane gets the viewport's cols x rows, not the window:
   1. First WxH = window + (viewport − pane), per dimension.
   2. When the layout has settled (no new snapshot of the pane for 300 ms, at most 2 s), it reads the pane's size again. Each `%layout-change` of the window already brings a new snapshot.
   3. If the pane is still off, it corrects WxH by the measured ratio of pane change to window change. At most three corrections per viewport or watch.
   4. WxH is at least the viewport and at most 4 × the viewport, per dimension.
   5. It stops when the window is not at WxH: another client is latest, or the layout cannot grow the pane.
   With tmux-agents (28-column sidebar, two work panes of equal share), a 180-column viewport gives a 390-column window, and the agent pane is 180 columns.
4. **Typing.** On `keys`, `text`, `bytes` or `paste` to the watched pane, the bridge asks tmux for the window's size, beside the input and without delaying it. When the window is not at WxH, it sends `switch-client -E -t <session> ; refresh-client -C WxH`. That makes it latest again, as a key press makes a desktop terminal latest.
5. **A new viewport** recomputes WxH and makes the bridge latest again, as a terminal resize does for a desktop client.
6. **Unwatch, a watch of a pane in another session, a watch of a window that is not `latest`, or a pane gone:** `refresh-client -f ignore-size ; refresh-client -C WxH`, before any `switch-client`. The window goes back to a desktop client that views it. When no other client is attached anywhere, a client that once had a size still counts (the hazard in the findings), so the windows of its session keep that size.
7. **The client leaves:** the bridge exits and its control client detaches. Where a desktop client views the session, the desktop's size returns at once. Elsewhere the windows keep the device's size until a desktop client views them (fact 5).

### What stays protected

- **Invariant 1 is unchanged.** Tests and repro scripts use `-L` servers only.
- **The daemon's client** stays `ignore-size,no-output` and never sends a size (ADR 0019, fact 8).
- The bridge never sends `resize-pane`, `resize-window` (it would pin the window to `manual`), `select-window`, `select-pane` or `refresh-client -A '%p:off'`. `switch-client` gets a session id only, so it selects no window or pane.
- The bridge never changes `window-size`, `aggressive-resize` or any other option. The facts show no option change is needed: tmux's default is `latest`. A user who chose another value keeps today's behaviour.
- The only sizes the bridge sends come from a validated `viewport`.
- No pane content reaches the logs. A resize is logged with ids and sizes.

### Limits (accepted)

- Two devices on one window: the latest one gets its size; the other scales the pane to fit, as today.
- `switch-client` makes the bridge latest only for its session's current window. When the watched pane is in another window, and a desktop client views the same session, the window keeps the desktop's size, and the device scales it. `aggressive-resize on` has the same effect.
- With no other client on the session, every window of it takes the device's size, also the ones the device does not show.
- The pane fill is exact only when the layout lets the pane grow. Otherwise the device scales what it gets.

### Plugins

- `client-attached` hooks still see a control client with no size (80 columns). tmux-agents and other plugins that skip control clients are unaffected.
- `window-resized` hooks run on every resize the device causes. tmux-agents' `fit` keeps its sidebar width and the work panes' shares (fact 9). A plugin that changes the layout by window width now sees the device's width.
- `refresh-client -C` runs no `client-resized` hook.
- README "Plugins that react to every client" says this.

### Apps

- iOS and Android compute cols x rows for the pane area at a default font. Regular size class or Medium/Expanded window: 13 pt (sp). Compact: 10 pt (sp). Never below the 9 pt (sp) floor. The cell size is measured the way each renderer measures it (`CellMetrics` on iOS, JediTerm's on Android).
- They send `viewport` before `watch` and after every change of the pane area, debounced by 300 ms.
- They render the `screen` they get, with today's fit modes. When the pane matches the viewport, `.whole` gives the default font.
- The demo bridges honour `viewport`: the demo pane takes the viewport's size.

## Consequences

- On an iPad, a task pane fills the screen at a readable font. On a phone, it is a phone-sized terminal, not a scaled-down desktop.
- The desktop layout changes while the user works on a device. It comes back when the user types on the laptop, or when the device leaves. This reverses the old invariant 2.
- Windows of a session that only the device viewed keep the device's size until the laptop views them.
- A window can be much larger than the device: the other panes of a split share the extra width.
- A user with `window-size smallest`, `largest` or `manual` sees no change.

Rejected:
- **`ignore-size` forever** (ADR 0001). Panes stay unreadable on the device, and 80x24 task sessions stay tiny.
- **Only fixing the size at creation** (`new-session -x -y`). It helps only sessions that muxa creates. Sessions made on the laptop, and every pane after a desktop resize, stay as they are.
- **Only resizing windows no desktop client views.** The device would still get the desktop's size whenever the laptop is on the same session, and the bridge would have to guess who is looking.
- **The window at the viewport's size.** In a split, the pane gets only its share: tmux-agents' agent pane would be 75 of 180 columns, and the device would show it small or with empty space.
- **Per-window sizes** (`refresh-client -C @id:WxH`). They cap the window in every mode, so typing on the laptop could not take it back (fact 7). They also need tmux 3.3.
- **`resize-window` or `resize-pane`.** `resize-window` pins the window to `manual`, so the laptop could not take it back. `resize-pane` changes the user's split.
- **Changing `window-size` or `aggressive-resize`.** They are the user's options, and the default already works.
- **`send-keys -K` to become latest.** It needs tmux 3.4, and runs the key through the client's key bindings.

## Implementation plan

| Step | What | Verified by |
|---|---|---|
| 1 | Repro scripts and findings rows | `make repro` on 3.2a, 3.4 and the latest |
| 2 | This ADR, AGENTS.md invariant 2, `docs/plan.md`, README | review |
| 3 | `viewport` in `PROTOCOL.md`, fixtures, Rust, Swift and Kotlin types; the bridge's sizing | `make check`, `make test-tmux` with a pty desktop client: the device's viewport resizes the window, desktop typing takes it back, disconnecting gives it back, the daemon's client resizes nothing |
| 4 | iOS and Android send `viewport`; the demo bridges honour it | unit tests for the viewport math; `make test-app`, `make test-android-app`: the pane fills its area |
