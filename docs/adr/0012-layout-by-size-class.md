# 0012: Layout by size class, and a readable terminal on a phone

## Status

Accepted. Extends ADR 0004 (fit modes).

## Context

- The app ran on iPad only. On an iPhone the split view collapses to a stack, the pane bar has no room, and a desktop pane is far wider than the screen.
- The pane's grid never changes (ADR 0001, 0004). A 100-column pane on a 440 pt wide phone gets about a 6 pt font in `.width`, too small to read.
- The iPhone Duo folds. Its outer screen is 466 x 678 pt, its inner one 669 x 951 pt (Xcode 27.1's simulator). Folding changes the screen under a running app.

## Decision

- **The layout follows the size classes, never the device.** The phone layout is used when the width or the height is compact (`EnvironmentValues.phoneLayout`). A Max iPhone in landscape is regular wide but compact tall, so it keeps the phone layout. A folded Duo gets the phone layout, and an unfolded one gets the iPad layout.
- **The terminal fit follows the font, never the size class** (`PaneFit`). Automatic picks `.whole` while the whole pane is readable in a tall view, else `.width`. When even `.width` would go below 9 pt, it picks `.readable`: the font stays at 9 pt, and the view scrolls on both axes and follows the cursor. A pinch switches between `.whole` and `.readable`. The Terminal Size menu offers all four.
- **The chosen fit lives in `AppModel`**, so a rotation or a fold keeps it.
- **Phone layout:**
  - the pane is pushed over the sidebar when the user connects or opens a session (`preferredCompactColumn`);
  - the Chat / Agent / Shell picker goes after Back, and Documents moves into More;
  - the Reader opens on its document, with its list one Back away and Done on both.
- **A fold changes nothing but the layout.** The connection, the pane, its screen and the fit stay. SwiftUI moves the pane view to the new layout; it does not rebuild it. UIKit resigns the key input view during that move, so the view takes the keyboard back when its superviews changed and it is still in a window. A resign that leaves it in place, such as hiding the keyboard, stays.
- **The fold is tested with a DEBUG hook.** Neither XCTest nor `simctl` folds a simulator. With `-demo -foldable`, a strip on the left edge switches the app between the Duo's outer screen size with compact size classes and the full screen (`FoldTests`, on the iPad mini).
- **Android uses the same rule**, mapped onto window size classes: Compact width or Compact height = phone layout; Medium and Expanded = tablet layout. See ADR 0013.

## Consequences

- iPhone and iPad are one universal app. Once published with iPhone support, the App Store does not let it be removed.
- Agent panes open in Chat, which reflows to any width. It is the main phone view; the terminal is for when Chat is not enough.
- Keyboard shortcuts work with a hardware keyboard on an iPhone too. The simulator can report the Mac's keyboard, which hides the key bar; its UI test then skips.
- Rejected:
  - Reflowing the terminal text to the phone width. TUIs draw with cursor moves, and a reflow breaks them.
  - `UIScrollView` zoom. It scales the rendered layer, and the text goes blurry.
  - The picker in the title's place. The title menu over it takes its taps.
  - Double-tap to switch fits. It delays the single tap that focuses the pane.
