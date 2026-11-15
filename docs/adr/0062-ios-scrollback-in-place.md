# 0062: iOS reads a pane's scrollback ahead, so one drag goes into it

## Status

Accepted. It changes how a touch opens the in-place scrollback of e87f86b on iOS. The `history` request (PROTOCOL.md "History") does not change. Android keeps e87f86b's behavior for now.

## Context

- Since ADR 0020 and 0021 the window follows the device. A pane's grid fits the view, so the pane has nothing to scroll. Before, the desktop-sized grid was taller than an iPad, and dragging read through it.
- Older lines exist only in tmux's history on the host. The emulator keeps none.
- 3c4115d opened the scrollback in a sheet after a 60 pt pull. e87f86b showed it in the pane's place, but still only after a 60 pt pull and a round trip to the host. The drag that asked for it ended on the newest line, so a user saw a bounce and nothing older. Users still reported that the Agent view does not scroll.
- SwiftTerm's iOS view is itself a scroll view that draws only its viewport. A history region stacked above the live grid in one scroll view is not feasible.
- `history` has no `rows`, and the keyboard resizes the pane. Splitting a reply into "history" and "screen" parts would race with tmux's reflow.

## Decision

1. The app reads the shown pane's `history` when the pane opens or resets, and again when a drag or a wheel scroll starts after new output. The first drag does not wait for the host.
2. A drag down past the grid's top by 24 pt shows the latest snapshot over the pane, at its newest line. The rest of the same drag scrolls into older lines.
3. The snapshot is read-only and frozen. Output does not move it, and a newer reply waits until the user is back on the live pane.
4. Coming back to the newest line, a drag past it, a tap, the Live button, a key or a paste returns to the live pane. Escape only leaves, as in e87f86b.
5. Without a snapshot yet, the pull falls back to e87f86b's scrollback, which loads and shows any error. More → Scrollback stays as it is.

## Consequences

- While reading back, the user does not see live output. Live and the newest line are one gesture away.
- The snapshot may miss the lines printed between the last read and the drag. The live pane shows them.
- The app reads `history` more often: once per new screen and once per drag after output.
- iOS and Android differ until Android gets the same change.
- With an alternate-screen agent, tmux's scrollback holds the primary screen (PROTOCOL.md "History"), not the agent's output.
