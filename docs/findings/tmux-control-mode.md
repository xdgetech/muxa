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

## Answered in Phase 3

| Fact | Test |
|---|---|
| No notification arrived inside a `%begin`/`%end` block across 200 snapshots taken while another pane streamed output. The parser needs no defensive handling. | `crates/muxa-tmux/tests/tmux_smoke.rs` `notifications_never_arrive_inside_reply_blocks` |
| Hooks triggered by our own `switch-client` (`display-message`, `run-shell`) do not put blocks into our reply queue. | `crates/muxa-tmux/tests/tmux_smoke.rs` `hook_output_does_not_misalign_replies` |
| When the session a control client is attached to is destroyed, tmux detaches the client (`detach-on-destroy`, default on). The bridge sends `pane-gone`, then `bye`. The client reconnects. Reattaching inside the bridge is an M3 item. | `crates/muxa-bridge/tests/bridge.rs` `follows_the_pane_across_sessions_resizes_and_exit` |
| A snapshot plus the stream after it reproduces tmux's screen and cursor exactly. Checked on 50 random cut points per run, with colors, cursor moves, CJK, erase-line, a scroll region and the alternate screen. | `crates/muxa-bridge/tests/stream.rs` |

## Emulator differences (client risk)

Found with vt100 as the test oracle. Both are emulator behaviors, not bridge errors: the bridge forwards the same bytes either way. SwiftTerm checked in `apple/MuxaKit/Tests/MuxaSessionTests/EmulatorTests.swift`.

| Behavior | tmux (and xterm) | vt100 0.18 | SwiftTerm 1.99 |
|---|---|---|---|
| Scroll region and the alternate screen | one region for the terminal | one region per screen; switching screens switches regions | one region per screen, like vt100 |
| Cursor after DECSTBM (`ESC[t;br`) | home of the screen (0,0) | top of the new region | (0,0), like tmux |

A TUI that sets a region and switches screens without resetting it can render differently on the client. Mainstream TUIs reset the region before switching. The snapshot sets the region after `1049h`, so a fresh `screen` is always right; only later output can hit this. The tmux side was checked with `printf '\033[3;5r\033[?1049h…'` in a 20x10 pane: the line feed scrolled the region on the alternate screen.
