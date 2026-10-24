# tmux control mode: verified facts

The bridge design relies on these facts. Each one has a script that checks it on an isolated `-L` server. Run them all with `make repro`; CI runs them on tmux 3.2a, 3.4 and the latest Homebrew release. Last checked locally on tmux 3.7c.

If a script fails on a new tmux version, the design must be revisited before release.

## Sizing

| Fact | Script |
|---|---|
| A control client attached with `-f ignore-size` never resizes a window. This holds next to a desktop client and when it is the only client. | `scripts/repro/ignore-size.sh` |
| **Hazard:** after `refresh-client -C <size>`, a lone `ignore-size` control client **does** size the window (here to 400x100). muxa never sends `refresh-client -C`. | `scripts/repro/ignore-size.sh` |
| Hooks see `#{client_width}` = 80 for a control client on `client-attached`. A plugin that hides panes on narrow clients, such as the tmux-agents sidebar, changes the desktop layout. Such plugins must skip control-mode clients (`#{client_control_mode}`). | `scripts/repro/plugin-client-width.sh` (set `MUXA_REPRO_PLUGIN` to check a real plugin) |

The `refresh-client -C` workaround for narrow-client plugins was considered and rejected because of the hazard above. It resized every window as soon as the desktop client had left.

## Output stream

| Fact | Script |
|---|---|
| `%output` arrives only for panes in the control client's current session. After `switch-client`, it arrives for the new session's panes. | `scripts/repro/output-scope.sh` |
| `%output` escapes only bytes below 0x20 and `\` (as `\ooo`). Every other byte, including invalid UTF-8, is passed raw. Read lines as bytes. | `scripts/repro/output-raw-bytes.sh` |
| While a program uses the alternate screen, `capture-pane` shows the alternate screen and `capture-pane -a` shows the primary one. | `scripts/repro/alt-capture.sh` |

## Commands

| Fact | Script |
|---|---|
| In an `a ; b ; c` line, each command gets its own `%begin`/`%end` block. After a runtime `%error`, the rest of the line is skipped and sends no block. A parse error gives one `%error` block for the whole line. | `scripts/repro/list-error.sh` |
| Name targets are ambiguous: `-t b` resolved to window `bash` in the current session, not to session `b`. Always target by id (`$n`, `@n`, `%n`). | `scripts/repro/name-targets.sh` |
| A double-quoted argument with `\"`, `\$`, `\\` and `\ooo` escapes carries any text intact. `send-keys -H` passes raw bytes, including UTF-8. | `scripts/repro/input-encoding.sh` |

## Open questions (tested in Phase 3)

- Can a notification arrive inside a `%begin`/`%end` block on tmux 3.2–3.7? The man page says never. The tmux 3.8 changelog mentions queuing notifications so they are not sent inside blocks.
- Do blocks from hooks that our own commands trigger (for example `client-session-changed` after `switch-client`) arrive flagged as ours (flags `1`)?
