# 0079: A split is a shell of the same Task; a Task closes from Focus

## Status

Proposed (2026-11-23). Decision 3's tab close button changed in ADR 0097: it hides the tab. Amends ADR 0070 decision 1 (multi-pane Focus) and ADR 0078 decision 1 (`⌘W`). Wire: `protocol/PROTOCOL-v2.md`, `task-shell`, `close-terminal`, `close-task`.

## Context

The Mac's split put a second Task beside the first. In use, the developer wanted what iTerm2 does: a new shell in the same place, beside the agent, to run tests or git while the agent works. `⌘W` closed the whole window, not the pane. A pane the Mac made by itself would not show on the developer's other devices. Focus also had no way to close a Task: only Approve on a Review item ended one.

## Decision

1. **A split starts a shell of the open Task on the host.** `task-shell` runs the user's login shell in the Task's working place, labelled `muxa.role = shell` with the agent's `muxa.dir`.
   - Every client finds a Task's shells by that label and lists them with the Task. The Mac shows them as panes beside the agent. iOS and Android show a row to pick the agent or a shell.
   - A Task has at most 3 shells. With the agent, four panes fit side by side.
   - A shell gets no connections or MCP servers. Secrets go only into an agent's environment (ADR 0069).
2. **`⌘W` in Focus closes the active shell, as in iTerm2.** `close-terminal` ends only a Task's shell.
   - On the agent's pane `⌘W` does nothing. The window stays, so a stray `⌘W` costs nothing.
   - Outside Focus, `⌘W` closes the key window as before.
3. **A Task closes from Focus, after the user confirms.** Each tab and each Task row in the sidebar has a close button, the row's context menu and the Pane menu have Close Task….
   - `close-task` does what Approve does without an item: the same check for work that would be lost, then the agent, its loop and its shells end, and clean, merged branch worktrees go.
   - The Task's open Inbox items become Done.
   - When work would be lost, the app asks again with the host's reason before it sends `force`.
4. **On the Mac, the Reader lies over the whole window and Chat takes the agent's pane.**
   - The Reader opens from the right. Dragging its left edge makes it wider or narrower, a button fills the window, and Esc closes it. The width is kept on that Mac.
   - Chat replaces the agent's terminal in its pane, and the same button or `⌘⇧C` switches back. The Inbox stays a side panel.

## Consequences

- Closing a Task, by Approve or `close-task`, ends its shells too.
- Two Tasks side by side are gone from the Mac. Tabs and `⌃Tab` switch between Tasks.
- A shell that the user ends with `exit` stays as an ended terminal until the Task closes. Clients do not show ended terminals.
- Alternatives not taken:
  - Splits as local panes on the Mac: other devices would never see them.
  - A shell with the agent's environment: the shell would get the Workspace's secrets, against ADR 0069.
  - `⌘W` on the agent closes the Task: one key would end an agent's work. Closing asks first instead.
