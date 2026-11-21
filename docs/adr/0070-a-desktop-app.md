# 0070: The desktop client is a muxa app

## Status

Proposed (2026-11-21). Supersedes ADR 0017, 0032 and 0038 once built. Changes ADR 0035.

## Context

ADR 0017 made tmux itself the desktop client: muxa for tmux adds a task tree and popups, and "a dedicated desktop app would take that away". Under ADR 0067 the default backend is herdr-core, which has no tmux key table, no panes and no status line. Home, Focus, the Reader, Capture's global hotkey and the wide detail views of `docs/next.md` need a real window.

## Decision

1. **The desktop client is a muxa app for macOS.** It starts from the iPad app's code (SwiftUI, MuxaKit) and adds desktop parts: a menu bar, `⌘` shortcuts, the Capture hotkey window, and multi-pane Focus.
2. **Keys follow VS Code, not tmux.** `⌘` keys never reach programs in the terminal, so they do not clash with `⌃` keys there. The full table is in `docs/next.md`.
3. **It talks to the muxa server like the other apps**, over SSH exec to the same host, or over the local socket when it runs on the host.
4. **muxa for tmux (`tmux/`) is removed** with the tmux code (ADR 0067). Its New Task, Inbox and Ask move to the app and to `muxa capture` in a terminal.

## Consequences

- Desktop work moves from bash and tmux popups to Swift, shared with iOS.
- ADR 0032 and 0038 are superseded with muxa for tmux.

## Not decided

1. Whether a Linux desktop client is needed.
