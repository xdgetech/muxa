# ADR 0001: Read panes through tmux control mode with `ignore-size`

Status: accepted (2026-10-25)

## Context

muxa shows one existing tmux pane full-screen on a tablet. The desktop layout must not change: no resized panes, no restarted processes. A tmux pane has one PTY with one size, so it cannot have a different size per client.

Options considered:

1. Attach a normal tmux client on a PTY (what Multiplex and Remux do). The client's size takes part in window sizing, so the desktop layout changes.
2. Poll `capture-pane` and inject `send-keys`. This is simple, but polling misses intermediate frames, and the screen state is only as fresh as the last poll.
3. `pipe-pane`. A pane has only one pipe, it has no initial screen, and it would compete with users who already use it.
4. Control mode (`tmux -C`), as iTerm2's tmux integration uses. It streams `%output` per pane and has command replies in the same ordered stream.

## Decision

The bridge attaches as a control-mode client with `-f ignore-size`. It never sends `refresh-client -C`, `resize-*`, `select-window` or `refresh-client -A %pane:off`.

To show a pane:
1. Take a snapshot: pane state, `capture-pane -e` of the screen, `-a` for the primary screen, and `-P -C` for a pending escape sequence. All are sent in one command line.
2. Then forward the pane's `%output` from the snapshot's position in the stream onward.

The client emulates a terminal of the pane's exact size and scales it to fit.

## Consequences

- The desktop layout never changes. `scripts/repro/ignore-size.sh` proves it, and `scripts/layout-snapshot.sh` checks it on a live server.
- The tablet shows the pane at the desktop's size, scaled. Text can be small when the desktop pane is large.
- The bridge must switch its control client to the pane's session (`%output` scope). It must track the snapshot/stream cut exactly.
- Plugins that react to every client attach can misbehave. See `docs/findings/tmux-control-mode.md`.
- Minimum tmux is 3.2 (`-f ignore-size`, `list-* -f`).
