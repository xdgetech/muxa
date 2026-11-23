# 0080: Each device picks its terminal colors and font; the apps bundle JetBrains Mono and the Nerd Font symbols

## Status

Proposed (2026-11-23).

## Context

The apps drew every terminal with fixed colors (the mocks' background and SwiftTerm's ANSI colors) and the system monospace font. The developer's local terminal is Ghostty on its defaults, so muxa's terminals looked different. Ghostty also draws the Nerd Font icons that prompts and TUIs print, such as the GitHub and git branch icons, because it bundles the Nerd Font symbols. muxa showed empty boxes for them.

## Decision

1. **Settings › Terminal, on each device.** The user picks the colors, the font and, on the Mac, its size. The choice stays in that device's UserDefaults and never goes to the host.
   - Built-in colors: muxa (as before), Ghostty's defaults, Solarized Dark and Dracula.
   - Import: an iTerm2 `.itermcolors` preset, or a Ghostty theme file (`background`, `foreground`, `cursor-color`, `selection-background`, `palette = N=#rrggbb`). `MuxaSession.TerminalTheme` reads both and is tested in `make test-apple`.
   - Fonts: the bundled JetBrains Mono, the system monospace and, on the Mac, every installed monospaced font. iOS has no user fonts, and its size still comes from the fit to the terminal's grid (ADR 0067).
2. **The apps bundle JetBrains Mono (SIL OFL 1.1) and Symbols Nerd Font Mono (MIT)**, fetched by `scripts/vendor-fonts.sh` at pinned versions with sha256 checks. JetBrains Mono becomes the default font.
3. **The symbols font is the fallback of the picked font**, as a Core Text cascade list.
   - Checked on macOS: a cascade list on a named font (JetBrains Mono, Menlo) reaches the symbols font, also in the bold variant SwiftTerm makes.
   - The system monospace font ignores a cascade list, so it shows no icons. Settings says so.

## Consequences

- Terminals look the same as before only with the muxa colors and the system font. The new default font changes the look for everyone.
- Each device keeps its own look. The Mac and the iPad do not share it.
- Android is not changed yet. Its JediTerm view keeps its own colors and font.
- The apps grow by about 3.6 MB of fonts.
- Alternatives not taken:
  - Read the local terminal's own settings: one app cannot read another's, and iOS and Android have no local terminal.
  - Keep the look on the host for every device: the Mac and a phone want different sizes, and the wire would carry display settings.
  - Bundle a patched Nerd Font in place of JetBrains Mono plus the symbols: the symbols then work only with that one font.
