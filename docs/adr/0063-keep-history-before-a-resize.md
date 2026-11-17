# 0063: The bridge keeps a pane's history before it resizes the window

## Status

Accepted. It extends `history` (PROTOCOL.md "History") without a wire change.

## Context

- Since ADR 0020 the bridge sizes the watched window for the device. Agent UIs redraw after a resize, and some print CSI 3J, which makes tmux drop the pane's whole history (`scripts/repro/clear-history.sh`).
- So opening an agent pane on a device cleared the lines a user wanted to scroll back to. The app's Scrollback showed only the screen. Before ADR 0020 the window kept the desktop's size and nothing was cleared.
- tmux has no option to ignore CSI 3J in a pane.

## Decision

1. Before the bridge takes, or gives back, a window for the device, it reads the scrollback of each pane in the window (`capture-pane`, up to 10000 lines) and keeps it in memory.
2. A later read is joined to what was kept. When it still holds the kept lines' last three non-empty lines, only the lines after them are added. Otherwise the history was cleared between, and all of the read follows.
3. `history` sends the joined lines: the newest that fit in 8 MiB, with `truncated` when some were left out.
4. The kept lines live as long as the bridge process. They never reach logs (AGENTS.md invariant 5).

## Consequences

- A user scrolls back past a redraw that cleared tmux's history, while the device stays connected.
- A pane redrawn while no device bridge was running loses what tmux lost.
- An agent that redraws its whole conversation after the clear may show some lines twice.
- Each resize the bridge makes costs one `capture-pane` per pane of the window.
