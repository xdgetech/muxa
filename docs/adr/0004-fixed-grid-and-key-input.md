# ADR 0004: Fixed-grid terminal view and app-owned key input

Status: accepted (2026-10-25)

## Context

The client must show a pane at its real cols x rows and never resize it. SwiftTerm's `TerminalView` derives cols x rows from its frame and has no fixed-grid API. Its `processSizeChange` is internal. SwiftTerm's own key handling sends bytes through the same delegate call as its replies to host queries, which tmux already answers.

## Decision

- `PaneTerminalView` (`apple/MuxaKit/Sources/MuxaUI`) searches for the largest font whose cell grid fits. It computes cells the way SwiftTerm does. It sets the terminal view's frame to exactly cols x rows cells, with less than one cell of slack, and sets the font last. SwiftTerm then derives exactly the pane's size.
- Two fit modes, and neither changes the grid. `.whole` fits every row and column in the view. `.width` sizes the font from the columns only. The grid is then taller than the view, and an outer scroll view shows part of it. The scroll view follows the cursor unless the user has scrolled the cursor out of view. Views wider than they are tall use `.width` by default. A menu in the pane bar overrides the choice.
- Every byte SwiftTerm wants to send to the host is dropped.
- `KeyInputView` is the first responder. Named keys and chords become tmux key names (`KeyMap`), sent with `send-keys`, so tmux encodes them for the pane's modes. Typed text goes through `insertText` and is sent literally. Cmd chords stay with the app.
- The Xcode project is generated with XcodeGen from `apple/Muxa/project.yml` and is not checked in.

## Consequences

- If SwiftTerm changes how it measures cells, the grid can be off by one. A fallback calls `resize(cols:rows:)` when the derived size differs.
- `UIKeyInput` has no marked text, so IME composition (for example Japanese input) is not supported yet. It needs `UITextInput`.
- Contributors need XcodeGen (`brew install xcodegen`). A generated project avoids merge conflicts in `project.pbxproj`.
