# 0073: The iOS layouts follow the design mocks

## Status

Proposed (2026-11-22). Builds on ADR 0012, 0066 and 0070.

## Context

The iPad and iPhone app showed Home and Focus as two light-mode split views with text buttons. The design mocks (PhoneInbox, PhoneSpaces, PhoneTab, Tablet) show a dark app, a phone with tabs, a tablet whose sidebar switches between the Inbox, the Tasks and the Routines, and icon buttons.

## Decision

1. **Dark only.** The app forces dark appearance. The palette is the mocks' (`Muxa/Theme.swift`). A light palette would be a second design that no mock shows.
2. **The phone has four tabs:** Inbox, Workspaces (the Tasks by Workspace), Routines and Settings (the hosts). A Task opens over the tabs with Chat, Terminal and Reader as segments.
3. **The tablet merges Home and Focus.** The sidebar's segments are Inbox, Tasks (by state) and Routines; the main area shows the picked item, Task or Routine. `NextNavigation.focus` means "the Tasks segment" there.
4. **The terminal stays attached under Chat and the Reader.** The segments lay Chat and the Reader over the terminal. Its grid keeps its size, and Chat's answers reach the agent through the shown terminal's channel.
5. **One Approve, named by the agent.** The Needs-you bar and the reply bar offer the first option of the agent's menu that does not cancel, with the option's own label ("Allow"). Only the user picks it (ADR 0009). Chat shows every option.
6. **The Mac keeps Home and Focus** (ADR 0070). It shares the palette and the shared views; its Home and Focus live in `MuxaMac/`.
7. **Left out:** the mocks' slots, queue, memory, pane counts, "Link to Task", Edit in the reply bar, and the share-sheet capture. muxa has none of them.

## Consequences

- iOS-only layouts live in `Muxa/Next/Touch/`, which the Mac target excludes.
- `NextTests` reach the Tasks and Routines through the phone's tab bar or the tablet's segments.
- The persistent key row on the phone sends keys that need no state. Ctrl, which waits for the next key, stays on the keyboard's own bar.
