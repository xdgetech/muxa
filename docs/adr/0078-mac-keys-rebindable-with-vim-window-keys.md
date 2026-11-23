# 0078: The Mac's keys start as iTerm2 and VS Code, can be changed, and can add Vim's window keys

## Status

Proposed (2026-11-23). Amends ADR 0070 decision 2. ADR 0079 changes `⌘W` in Focus and what a split is.

## Context

ADR 0070 gave the Mac VS Code's keys. Split pane was `⌘\`. In use that key did not work: 1Password takes `⌘\` for itself by default, and on a JIS keyboard `\` needs `⌥¥`. Focus also had no key to move between panes or to close one. The developer works in Vim and wants Vim's `CTRL-W` window commands for panes.

## Decision

1. **The default pane keys follow iTerm2.** VS Code's keys stay where they do not clash.
   - Split pane `⌘D`, next and previous pane `⌘]` and `⌘[`, close pane `⌥⌘W`, Close Other Panes with no key. `⌘1`…`⌘9` stay.
   - The Pane menu's keys work only in Focus. Home keeps `⌘D` and `⌘[`/`⌘]` for its own use (`docs/next.md`).
   - `⌘W` still closes the window, as in every Mac app.
   - No function keys by default: on a laptop they need Fn. Next Task that needs you is `⌘J`, the previous one `⌘⇧J` (ADR 0066 had `F8`).
2. **Every menu key can be changed on each Mac.** Settings › This Mac › Keyboard records a new key per shortcut and shows when two use the same key.
   - The keys live in UserDefaults on that Mac. They do not go to the host.
   - A recorded key needs `⌘`, `⌃` or `⌥`. An F-key alone is fine. A plain letter would fire while the user types.
   - The keys a view handles (`⌘⏎`, `⌘L`, `⌘1`…`⌘9`, and the like) stay fixed.
3. **Vim's window keys are an option, off by default.** With them on, a prefix (default `⌃W`) and one key run a pane command while a terminal in Focus has the keyboard.
   - The keys after the prefix: `v`/`s` split, `h`/`←`/`p` previous, `l`/`→`/`w` next, `W` previous, `c`/`q` close, `o` close the others, `1`–`9` a pane.
   - The prefix twice sends the prefix to the terminal, so a shell or Vim there still gets `⌃W`. Any other key cancels and types nothing.
   - The logic is `MuxaSession.WindowKeys`, tested in `make test-apple`.

## Consequences

- ADR 0070's rule that "`⌘` keys never reach programs in the terminal" still holds for the defaults. Vim's window keys break it on purpose for one key, the prefix, and only when the user turns them on.
- Panes sit side by side, so `s` splits beside the pane, not under it. Vim's `j`/`k` do nothing until Focus can stack panes.
- The menus, the shortcuts sheet and the key hints read the user's keys, so they cannot drift apart.
- Alternatives not taken:
  - Keep `⌘\` and add `⌘D` beside it: the Mac menu has one key per item, and the broken key would still be the one shown.
  - Vim's keys as the default: `⌃W` would stop reaching programs in the terminal for every user.
  - A keys file the user edits: Settings already holds this Mac's preferences, and a recorder needs no syntax.
