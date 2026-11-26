# 0097: Closing a tab keeps its Task

Status: accepted (2026-11-24). It changes ADR 0079 decision 3 for the Mac's tab strip.

## Context

- ADR 0079 gave each tab in Focus's top strip a close button that closes the Task, after the user confirms.
- The developer wants the tabs as a visual help only: "keep the tab as just visual help, and only close the task from sidebar, or other 'close task'".
- Closing a Task stops its agent and may remove its worktrees. A tab's × looks like a browser tab's, which only hides something.

## Decision

1. **A tab's close button hides the tab.** Nothing asks. The Task keeps running and stays in Focus's sidebar.
2. **Opening the Task again shows its tab again**: from the sidebar, Go to Task, an Inbox item or a pane. The active Task always has a tab.
3. **Closing the active tab opens its neighbour.** With no other tab, Focus goes back to Home.
4. **A Task closes only by Close Task**: the sidebar row's close button and context menu, the Pane menu's Close Task…, and Close on a Bot's chat. Each still asks first (ADR 0079).
5. Hidden tabs live in the Mac app's memory for this run. They are not saved and not sent to the host.

## Consequences

- A tab can no longer stop an agent by mistake.
- After a restart, every live Task has its tab again.

## Alternatives not taken

- **Keep the close on the tab and add Hide to its menu**: the button that looks harmless would still be the one that stops the agent.
- **Remove the × from the tabs**: the user asked to keep closing tabs, as a view change.
