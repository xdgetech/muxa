# 0092: ⌘B opens Bots on the Mac

## Status

Accepted (2026-11-24). Amends ADR 0086 (Bots `⇧⌘B`) and ADR 0070 (sidebar `⌘B`).

## Context

The user pressed `⌘B` to open Bots, and it did not. Two items shared the letter: Go › Bots (`⇧⌘B`) and Go › Toggle Sidebar (`⌘B`). In Focus `⌘B` hid the sidebar. In Home the sidebar item was disabled.

## Decision

1. Go › Bots is `⌘B`. B is for Bots, and the user reached for it first.
2. Go › Toggle Sidebar is `⌃⌘S`, the key Finder, Mail and Notes use.
3. Home and Focus keep `⇧⌘H` and `⇧⌘F`: `⌘H` hides the app and `⌘F` is Find.
4. Settings › This Mac › Keyboard still changes all three.

## Consequences

- Bots no longer follows the shift pattern of Home and Focus. Its hint shows `⌘B`.
- A user who changed one of these keys keeps their own key.
