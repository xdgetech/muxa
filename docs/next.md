# muxa next: Home and Focus

Status: proposed (2026-11-21). Decisions: ADR 0066–0070. Not in `docs/plan.md` Next until the developer orders it.

## In short

muxa becomes a place to keep and clear your own work list, with a focus mode for deep work in a terminal.

- **Home** is the main mode: the Inbox of things you have to do, and the Routines that feed it.
- **Focus** is for one Task: its panes, terminal and Reader.

The assistant layer goes: the secretary, staff, delegates, plans, policies and the Today queue (ADR 0066). herdr-core replaces tmux under a required muxa server (ADR 0067). The desktop gets a muxa app (ADR 0070).

## Concepts

| Concept | What it is | Where you see it |
|---|---|---|
| Inbox item | Something **you** have to do: Review, Reply, Decide, Read, To do, Failed | Home |
| Needs you | A running agent waits on you now (a question, an approval) | A strip in Home, the sidebar in Focus. Never an Inbox item |
| Task | One piece of work with its sessions and panes | Focus, and linked from items |
| Place | Where a Task works: a folder, a repo branch, a repo read only. A Task can have several | Under the Task's name |
| Routine | A scheduled agent run | Silent while it works. Its output or failure becomes an item |
| Capture | The way to add an item from anywhere | Every device |
| Workspace | A group of Tasks, Routines, places and connections | Settings, filters |

## The Inbox

### Kinds and sources

| Kind | Example | Source |
|---|---|---|
| Review | "Inbox UI is ready for review" | A Task entered review: its check passed, its agent opened a draft PR, or you moved it |
| Reply | "Reply to the auditor about access logs" | Capture |
| Decide | "An assumption broke: Q1 hiring plan" | Capture, or a Routine whose output asks for a decision |
| Read | "News digest: 6 items" | A Routine's output |
| To do | "Book the venue" | Capture with no kind word |
| Failed | "Delivery digest failed: gh login expired" | A Routine run that failed after its retries, or a Task whose agent failed |

A session that waits on you is not an item. It is short-lived and shows as Needs you. A turn that ends with nothing to review also shows there. A Task ready for review is an item, because it can wait for days. The full rules, and what each old kind becomes, are in ADR 0066.

### Order and due dates

- The list is in **your order**. You drag an item, or press `⌥↑` / `⌥↓`. New items go to the top.
- A due date is optional. Only items with one show it. An overdue date shows in red.
- Later hides an item until a time you pick. It then comes back on top.
- Filters: kind, Workspace, due, hidden by Later.

### Detail

The detail pane is the main part of the screen. It shows:

1. the kind, the position, the source and the due date;
2. the linked Task with its state and places;
3. tabs: Summary, then each document of the Task in the **Reader** (a plan, a draft, a run log), then Screenshots;
4. the actions, each with a key.

| Kind | Actions |
|---|---|
| Review | Approve (closes the Task), Request changes (sends a comment, the agent goes on), Open in Focus, Open PR |
| Reply, Decide, To do | Open Task or Start Task, Later, Done |
| Read | Make it a Task, Later, Done |
| Failed | Retry, Edit Routine or Open in Focus, Done |

## Routines

- Each Routine sets the kind of its output (Read, Decide or none) and when it makes an item (always, or only when its output says so).
- A failure after the retries always makes one Failed item.
- The Routines screen: a narrow list (Failed, Running, Paused, with the last 7 runs) and a wide detail with Overview, Brief (in the Reader, editable) and Runs (each run's output or log in the Reader).

## Tasks and places

A Task has one or more places:

| Place | Use |
|---|---|
| Folder | Work with no repo: a budget, a reply draft. Under `<workspace root>/tasks/<name>` |
| Branch | A worktree of a repo, for code changes |
| Read only | A repo's main checkout, for research |

Example: an audit Task has one folder for its notes and three repos read only.

## Devices

| Device | Starts in | Main use |
|---|---|---|
| Phone | Home | Clear the Inbox, answer Needs you, short replies in a Task |
| Tablet | Home | Both modes, with the Reader beside the list |
| Desktop | The last mode | Focus for long work, Home in the morning and between Tasks |

Layout on desktop and tablet: a narrow list on the left, a wide detail on the right. Home, Routines and Settings share it. On a phone, the list and the detail are two screens.

Focus on desktop: Workspaces and Tasks on the left with Needs you on top, the Task's panes in the middle, and the Inbox as a panel that is closed by default.

## Capture

| Device | Entry |
|---|---|
| Mac | A global hotkey (`⌃⌥Space`) opens a one-line window. A first kind word, `#workspace` and `due fri` are read from the text and shown as chips (rules in ADR 0066). `⏎` adds, `⌘⏎` adds and starts a Task |
| Mac, in the app | The field on top of Home (`⌘N`), the Inbox panel in Focus |
| Terminal | `muxa capture "…"`; it replaces muxa for tmux's `prefix + A` |
| iPhone, Android | The share sheet (from Slack, mail or a browser), the field on top of the Inbox, a widget, a shortcut. Android also gets a Quick Settings tile |
| iPad | The share sheet, and `⌘N` with a keyboard |

A shared link stays with the item. muxa does not read the linked service.

## Workspace settings

| Section | Contents |
|---|---|
| General | Root folder, repos, where folder Tasks go, sessions at once, Capture tag |
| Connections | Slack, Linear, Notion, GitHub, Calendar: the account, the access (read, create issue), who uses it, last use |
| MCP servers | Hosted or local servers, and which Tasks and Routines turn them on |
| Agents | Default agent, what Routines and Tasks may do |
| Memory | `.muxa/WORKSPACE.md` in the Reader |

Host settings sit beside them: server and backend, devices, shortcuts, resource limits. Connections follow ADR 0069.

## Keyboard (desktop)

Shortcuts follow VS Code, not tmux (ADR 0070). `⌘` keys do not reach programs in the terminal, so they never clash with `⌃` keys there.

| Action | Key |
|---|---|
| Home, Focus | `⌘⇧H`, `⌘⇧F` |
| Command palette, go to Task | `⌘⇧P`, `⌘P` |
| Next Needs you, next Task | `F8`, `⌃Tab` |
| Split pane, focus pane, sidebar | `⌘\`, `⌘1`…`⌘9`, `⌘B` |
| Capture, settings | `⌘N`, `⌘,` |
| Inbox: select, reorder, preview, open | `↑↓`, `⌥↑⌥↓` or drag, `Space`, `⏎` |
| Inbox: main action, later, done, due | `⌘⏎`, `⌘L`, `⌘⌫`, `⌘D` |
| Review: request changes, open PR | `⌘R`, `⌘O` |
| Failed: retry | `⌘R` |
| New Task | `⌘⇧N` |
| Routine: run now, edit brief, pause | `⌘R`, `⌘E`, `⌘.` (in Routines; `⌘R` means "run" there) |
| Switch detail tab | `⌘[`, `⌘]` |
| Inbox panel in Focus | `⌘⇧I` |

## The session layer

A long-running muxa server owns the terminals through a session backend (ADR 0067). herdr-core replaces tmux. Rex can replace herdr-core later, when its output stream is public. The verified facts are in `docs/findings/session-backends.md`.

## What goes away

- The secretary and its root session (ADR 0054, 0056).
- Staff, hand-offs and the lead (ADR 0051), and the delegate and staff parts of Codex-owned threads (ADR 0028).
- The overall assistant's plans, policies, corrections and hypotheses (ADR 0049, 0050), the Today queue (ADR 0057, 0059).
- Inbox items made from a session's need (ADR 0033): they become Needs you.
- tmux control mode, its sizing (ADR 0001, 0020, 0021) and muxa for tmux (`tmux/`, ADR 0017, 0032).

Removal follows ADR 0066's and 0067's order. Frozen code stays until each part is removed.

## Open questions

Decided on 2026-11-21:

- A Routine run is not a Task; it shows only in Routines.
- Needs you is pushed to the phone also while the app is closed.
- Existing Captures, Work items and Inbox items are dropped at the switch, not migrated.
- claude.ai connectors stay with each Workspace's Claude login for now. Later, muxa holds several connections of one service and owns their MCP config.
- The server is required on Linux hosts too.

Still open (each ADR lists its own under "Not decided"):

1. How push reaches a closed app: the relay, what it may see, who runs it.
2. Do Claude Code and Codex report OSC 7501 to herdr-core, or do they need hooks?
3. Do programs survive a herdr-core restart and a reboot?

## Mockups

Interactive mockups of every screen exist outside the repo, because they show the developer's own workspaces. Ask the developer for the link.
