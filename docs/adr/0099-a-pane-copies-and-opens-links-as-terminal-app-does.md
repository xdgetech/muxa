# 0099: A pane copies and opens links as Terminal.app does

Status: accepted (2026-11-25). Mac app only; iOS and Android are follow-ups.

## Context

- The user, on their own Mac: "codexを使っている時にAgent画面がCopyできなかったりClickしてURLをひらけなかったりする". With Codex, text in a pane cannot be copied and a URL cannot be opened.
- Codex 0.171 turns on the alternate screen and mouse reports 1000, 1002, 1003 and 1006. Claude Code turns on none of them. Checked by starting both in a pty and reading their output.
- SwiftTerm's AppKit view sends every press, drag and release to a program that asked for mouse reports. So a drag in Codex selects nothing. Only Shift bypasses it, and the program can take Shift too (`mouseShiftCapture`).
- SwiftTerm opens a link on ⌘-click and underlines it on ⌘-hover, but it sends the press to the program first. Codex gets a press with no release.
- Its default handler opens any URL with a scheme, and any path that exists on this Mac. The text comes from the host.

## Decision

1. **Option, Shift or ⌘ held at the press keeps the mouse in the app** for the whole press (`MacPaneView` in `apple/Muxa/MuxaMac/TerminalPane.swift`). Option and Shift drag to select. ⌘ clicks a link. The program gets nothing. Plain clicks, drags and the wheel still go to the program.
2. **⌘C and a right-click menu copy.** A right-click on a selection shows Copy and Paste, also when the program takes the mouse. Without a selection, a program that takes the mouse gets the right-click.
3. **Only `http`, `https` and `mailto` links open** (`MuxaSession.TerminalLink`), with NSWorkspace on this device. Any other link beeps. The same rule as Markdown links in the item detail and the Reader.

## Consequences

- Copy and ⌘-click work the same in Codex, Claude Code and a shell.
- A program cannot get an Option-, Shift- or ⌘-click. No agent muxa runs uses one.
- A URL that the program wrapped onto two rows is two links. SwiftTerm finds a link on one row only, unless the terminal itself wrapped it.
- iOS still has no selection or links in a pane. Its pane view takes no touches; keys come from `KeyInputView`.

## Alternatives not taken

- **Turn mouse reports off for agents.** Codex scrolls its history and clicks with the mouse. That would break it.
- **A menu item to turn mouse reports off**, as SwiftTerm suggests. The user must find it before each copy.
- **Open every link SwiftTerm finds.** A file path or a custom scheme from the host would open apps or files on this Mac.
