# 0035: The sidebar shows one mode, and the detail follows the selection

## Status

Proposed. The developer chose this shape on 2026-11-04. It decides the navigation that ADR 0025 left open. It changes the phone rule of ADR 0012 and the tablet presentation of ADR 0033 §4. It depends on ADR 0033 slice I4. Proposed ADR 0046 changes §5: on a host with `work:8`, `+` opens Capture, which can still start a Task.

## Context

- The sidebar stacks the demo row, three Inbox items, Inbox, Routines, New Task, the hosts and the manual sessions. The user reads five kinds of rows to find one.
- The Inbox opens as a sheet over the terminal. To read an item's evidence, the app closes the sheet, opens the Reader, then opens the sheet again.
- The terminal is the default detail. With Tasks, the first thing to do is to answer, not to type.
- Hosts and Routines are visited rarely, but they sit on a phone's first screen. The developer rarely changes hosts.

## Decision

1. **One mode at a time.** The sidebar shows **Inbox**, **Tasks** or **Sessions**, picked with a segmented control under the title. The device remembers the mode.
   - Inbox: the host's full list of items, in the host's order, with no fold.
   - Tasks: Routines as the first row, then all Tasks grouped by what they need (today's All Tasks).
   - Sessions: the manual sessions grouped by repo, with each repo's New Task and Ask (ADR 0025, 0031).
2. **The detail follows the selection.** The detail column shows an Inbox item, a Task, or a session. The Inbox sheet is removed. On a phone, the same screens are pushed, and Back returns to where the user came from.
3. **A session's detail is its pane.** It keeps the Chat / Agent / Shell switch the app has (ADR 0005, 0012), and `⌘J` and `⌘R` as before.
   - The first view depends on where the session was opened from:
     - A Sessions row, the switcher or `⌘1…9`: the session's last view. The first time, Chat when the agent has it, else Agent, else Shell.
     - An item's or a Task's **Chat**: Chat. An `ask` is answered there.
     - An item's or a Task's **Terminal**, or `⌘O`: Agent. A `failure` is read there.
     - A Routine run: Agent. The agent banner (ADR 0008): the item, else Chat.
     - An item never opens Shell first.
   - The item screen shows both buttons. Chat shows only when the agent has Chat (ADR 0009).
   - The sidebar row stays highlighted. The sidebar collapses as it does for any pane, so the pane keeps the width (ADR 0020). The sidebar never changes its mode by itself.
4. **Settings holds the hosts.** It has the host list, connect, add, edit, delete, reorder and Disconnect, plus Try the Demo, Leave Demo and the version.
   - A one-line status row under the mode control shows the connection (`● work-mac`: grey when off, a spinner while connecting) and opens Settings → Hosts.
   - While the demo runs, the row reads `● Demo [Leave Demo]`. A demo user starts on the list, not on a pane, so the way out must be on the list.
   - Not connected: the sidebar shows "Connect to <last host>" and "Hosts…". On first launch: "Add Host" and "Try the Demo".
5. **`+` is New Task.** It sits in the sidebar's toolbar next to Settings.
6. **At connect,** Inbox and Tasks mode stay on their list, also when the list is empty. Sessions mode opens the default pane as before.
7. **Keys.**
   - iPad: `⌘T` Inbox mode, `⌘L` Tasks mode, `⌘⇧S` Sessions mode, `⌘0` shows or hides the sidebar, `⌘O` opens the item's terminal, `⌘⇧L` Later, `⌘,` Settings.
   - Android: the same chords with `Ctrl+Shift`, and `Ctrl+Shift+Alt+L` for Later. Android has no key for Settings.
   - Moving between items, the main answer, and Return on a phone stay as in `docs/inbox.md`.

## Consequences

- The mode says what the sidebar holds, so there is one place to look. A phone's first screen is the Inbox list, `+` and Settings.
- An item's evidence and its session are one tap from the item. No sheet closes and opens again.
- A connection no longer shows the terminal first, except in Sessions mode. muxa's original use, your tmux panes from an iPad, keeps working in that mode.
- Switching hosts takes two more taps. This is accepted because it is rare.
- The demo and the showcase must play `work:3`, or Inbox mode shows nothing.
- The store screenshots change.
- ADR 0012: the pane is pushed over the sidebar when the user opens a session, or at connect in Sessions mode only.
- ADR 0008: the banner opens the item first.
- Alternatives not taken:
  - A host picker in the toolbar: hosts rarely change.
  - Routines in Settings: they run and have Run now, and Settings is for setup.
  - Two modes (Inbox and Sessions): All Tasks and Routines would need a link elsewhere, and a Task's detail would have no column.
  - A fold ("show more") in the Inbox list: nothing else sits below it.
  - A fourth Chat mode, or a second Chat / Terminal switch outside the pane: Chat belongs to the pane's switch (ADR 0012).
