# tmux control mode: verified facts

The bridge design relies on these facts. Each one has a script that checks it on an isolated `-L` server. Run them all with `make repro`; CI runs them on tmux 3.2a, 3.4 and the latest Homebrew release. Last checked locally on tmux 3.7c.

If a script fails on a new tmux version, the design must be revisited before release.

## Sizing

| Fact | Script |
|---|---|
| A control client attached with `-f ignore-size` never resizes a window. This holds next to a desktop client and when it is the only client. | `scripts/repro/ignore-size.sh` |
| After `refresh-client -C <size>`, a lone `ignore-size` control client **does** size the window (here to 400x100). `ignore-size` only steps aside for a client without the flag. A bridge that has sent a size stays a candidate while it is the only client (ADR 0020). | `scripts/repro/ignore-size.sh` |
| A session made with `new-session -d`, as a task launcher makes it, is `default-size` (80x24), not the size of a desktop client attached to another session. Its panes are split from 80x24: tmux-agents' sidebar, agent and shell come out 28, 25 and 25 columns wide. An `ignore-size` control client watching it changes nothing. It gets the desktop's size only once a desktop client views it. `-x`/`-y` at creation set its size. | `scripts/repro/detached-session-size.sh` (set `MUXA_REPRO_PLUGIN` to see tmux-agents' layout) |
| Hooks see `#{client_width}` = 80 for a control client on `client-attached`. A plugin that hides panes on narrow clients, such as the tmux-agents sidebar, changes the desktop layout. Such plugins must skip control-mode clients (`#{client_control_mode}`). | `scripts/repro/plugin-client-width.sh` (set `MUXA_REPRO_PLUGIN` to check a real plugin) |
| `paste-buffer -p` wraps the text in `ESC[200~` … `ESC[201~` only when the pane's program has turned bracketed paste on, and sends each LF as CR. A multi-line reply (ADR 0033 `redo`, `change-course`) then reaches Claude Code or Codex as one paste. A program without bracketed paste gets each line as its own Enter. | `scripts/repro/paste-bracketed.sh` |

## Sizing for the device in use (ADR 0020)

| Fact | Script |
|---|---|
| `window-size` defaults to `latest` (a server started with `-f /dev/null`). | `scripts/repro/sized-client-latest.sh` |
| With `latest`, a control client without `ignore-size` that sent `refresh-client -C WxH` sizes the window to exactly WxH (no status line) while it is the window's latest client. A control client that never sent a size sizes nothing. The resize runs `window-resized` hooks; `refresh-client -C` runs no `client-resized` hook. | `scripts/repro/sized-client-latest.sh` |
| `attach-session` and `switch-client -t <its own session>` make a control client the latest client of the session's current window. Its own `send-keys`, and a repeated `refresh-client -C`, do not. A desktop key press makes the desktop client latest and takes the window back. | `scripts/repro/sized-client-latest.sh` |
| `refresh-client -f ignore-size` gives the window back to a desktop client, but only at the next size calculation: send `refresh-client -C` again after it. `refresh-client -f '!ignore-size'` (3.2+) drops the flag again. When the sized client detaches, the desktop's size returns at once. | `scripts/repro/sized-client-latest.sh` |
| A sized control client sizes only windows of its own session. When no other client views that session, it sizes every window of it, also the ones it does not show. They keep that size after it leaves, until a desktop client views them. | `scripts/repro/sized-client-scope.sh` |
| `smallest` lets a sized control client shrink a window that the desktop client is using; `largest` keeps the larger desktop size; `manual` ignores clients. `resize-window` sets that window's `window-size` to `manual`. `#{window-size}` shows the value in effect for a window. | `scripts/repro/sized-client-scope.sh` |
| A per-window size, `refresh-client -C @id:WxH` (3.3+; 3.2a answers `%error`), caps that window in every mode, also while the desktop is latest. It cannot follow the device in use. | `scripts/repro/sized-client-scope.sh` |
| An `ignore-size,no-output` client that never sent a size (the daemon's) sizes nothing, also as the only client. | `scripts/repro/sized-client-scope.sh` |
| When the window grows, tmux spreads the new columns over side-by-side panes one at a time: 28/25/25 at 80 columns becomes 62/58/58 at 180. A pane reaches a given width only through the window. tmux-agents' `window-resized` hook then puts its sidebar back to `@agents_sidebar_width` and splits the rest by the panes' shares: 28/75/75 at 180, 28/180/180 at 390. | `scripts/repro/sized-client-layout.sh` (set `MUXA_REPRO_PLUGIN` for tmux-agents) |

Before ADR 0020, `refresh-client -C` was rejected as a workaround for narrow-client plugins because a lone `ignore-size` client with a size still sizes windows (Sizing, above). It resized windows once the desktop client had left.

## Sizing from a device session (ADR 0021)

| Fact | Script |
|---|---|
| `new-session -d`, then `link-window -d -s @W -t $G:` and `kill-window` of its first window, gives a session that holds only `@W`. The user's session keeps all its windows and its current window; its clients keep their view; no layout changes. Hooks run: `session-created` (with the first window), `window-linked`, `window-unlinked`, `session-closed`. | `scripts/repro/device-session.sh` |
| `list-sessions`, `list-windows -a` and `list-panes -a` show the session, and list the linked window and its panes once per session. A session option such as `@muxa_device` is visible from window and pane formats, so `-f '#{==:#{@muxa_device},}'` hides the session from all three. | `scripts/repro/device-session.sh` |
| A command with no target (`tmux attach`, `display-message`) picks the session switched to last. A `switch-client` into the device session makes it that session; a later `switch-client` to the user's session by another client makes the user's session the most recent again. | `scripts/repro/device-session.sh` |
| `kill-session` of the device session, with its control client attached, changes no window, layout or client of the user's session. The session's own `detach-on-destroy on` detaches that client; under the user's `detach-on-destroy off` without it, the client moves to another session. `kill-session` of the user's session leaves the linked window alive in the device session. | `scripts/repro/device-session.sh` |
| A grouped session (`new-session -t`) holds every window of its target; it cannot hold one window. | `scripts/repro/device-session.sh` |
| tmux-agents' hooks run for a device session and add, remove or resize no pane of the user's session. | `scripts/repro/device-session.sh` (set `MUXA_REPRO_PLUGIN`) |
| With `window-size latest`, a control client in the device session that drops `ignore-size`, switches to it and sends `refresh-client -C` sizes the linked window, also when it is not the desktop's current window. The desktop's session, current window, its window's size and layout stay. Holds with `aggressive-resize` off and on. | `scripts/repro/device-session-size.sh` |
| A desktop key press in another window leaves the device's window as it is. In the same window it takes the window back; the device client's `switch-client` takes it again. | `scripts/repro/device-session-size.sh` |
| `refresh-client -f ignore-size` (then `-C`) gives the window back at once with `aggressive-resize off`. With it on, the window keeps the device's size until a desktop client views it. | `scripts/repro/device-session-size.sh` |
| A second device session for another window leaves the first window alone. A grouped session's client may not: with `aggressive-resize off` it is a candidate for every window of the group, and may size each window whose latest client is gone. It does in most runs, not all. | `scripts/repro/device-session-size.sh` |
| The daemon's `ignore-size,no-output` client on the user's session sizes nothing meanwhile. | `scripts/repro/device-session-size.sh` |
| Two device sessions can link the same window (two app windows or two devices, ADR 0029). The device client that switched last sizes it; the other one's `switch-client` takes it back. Neither session, nor the desktop's session, current window, size or layout, changes. When the latest one gives the window up (`ignore-size`, then `kill-session`), the window takes the remaining device's size at once, not the desktop's. Holds with `aggressive-resize` off and on. | `scripts/repro/two-device-sessions.sh` |
| `destroy-unattached on` would end a device session with its last client, but it crashed tmux 3.7c (SIGSEGV, `server_client_lost` → `notify_client` → `server_client_get_pane`, and the same from `cmdq_next`) when the session ended while its control client was still exiting. Seen in macOS crash reports from `crates/muxa-bridge/tests/viewport.rs` runs; not reproducible on demand. muxa does not use it: it kills the session itself. | crash reports (not scripted) |

## Output stream

| Fact | Script |
|---|---|
| `%output` arrives only for panes in the control client's current session. After `switch-client`, it arrives for the new session's panes. | `scripts/repro/output-scope.sh` |
| `%output` escapes only bytes below 0x20 and `\` (as `\ooo`). Every other byte, including invalid UTF-8, is passed raw. Read lines as bytes. | `scripts/repro/output-raw-bytes.sh` |
| While a program uses the alternate screen, `capture-pane` shows the alternate screen and `capture-pane -a` shows the primary one. | `scripts/repro/alt-capture.sh` |
| `capture-pane -p` without `-e` prints the screen as plain text, with no color escapes. `-J` joins a line the terminal wrapped. `screen_shows` relies on this to look for a fixed text, such as Claude Code's trust dialog. | `scripts/repro/screen-text.sh` |
| On a control client, `capture-pane -p -e -S -<n>` sends the last `n` scrollback lines, then the screen, with raw escape bytes for colors. The pane stays out of copy mode. `history_size` counts the lines the pane keeps. The `history` request relies on this. | `scripts/repro/history-capture.sh` |

## Commands

| Fact | Script |
|---|---|
| In an `a ; b ; c` line, each command gets its own `%begin`/`%end` block. After a runtime `%error`, the rest of the line is skipped and sends no block. A parse error gives one `%error` block for the whole line. | `scripts/repro/list-error.sh` |
| Name targets are ambiguous: `-t b` resolved to window `bash` in the current session, not to session `b`. Always target by id (`$n`, `@n`, `%n`). | `scripts/repro/name-targets.sh` |
| A double-quoted argument with `\"`, `\$`, `\\` and `\ooo` escapes carries any text intact. `send-keys -H` passes raw bytes, including UTF-8. | `scripts/repro/input-encoding.sh` |
| `new-session -e KEY=VALUE` (tmux 3.2+) reaches the pane's process byte for byte, through the same quoting. It overrides the global environment for that session only, leaves the global environment unchanged, and reaches later windows of the session. | `scripts/repro/session-env.sh` |
| `respawn-pane -k -t %n -c DIR -e KEY=VALUE`, sent on a control client's stdin, replaces the pane's process in place. The pane id, the pane's size and the window layout stay the same. No window or pane is selected, and no attached client moves. Without a command it reruns `#{pane_start_command}`. `-e` and `-c` reach the new process, and the session's `new-session -e` values stay. A pane started as a plain shell has an empty start command, so it gets a fresh shell: an agent that was typed into it must be typed again. | `scripts/repro/respawn-pane.sh` |
| `new-session -d` with multiple command arguments directly executes argv, preserving literal tilde, `$`, quotes, backslashes, UTF-8 and empty arguments. Its session environment and working directory reach the new process, and the original session/pane/layout stay unchanged. `new_session_command` rejects empty/NUL commands before dispatch. Local result: tmux 3.7c; supported-version CI remains required. | `scripts/repro/worker-respawn.sh` (`worker_new_session_preserves_native_arguments_and_existing_layout`) |
| Explicit-command `respawn-pane -k` sequences (default → A → B → edited-A → default), and no-command repeats after each, preserve argv bytes, workspace env, pane/layout and a real desktop client's view of another window. The public `respawn_pane_command` API covers a single shell-command string and multiple direct-exec argv entries; empty/NUL commands are rejected before dispatch. Old hook state/timestamp/session-id pane tags survive process replacement; they are not evidence of the new process. Local W0 result: tmux 3.7c; cross-version results still required. | `scripts/repro/worker-respawn.sh` (`crates/muxa-tmux/tests/tmux_smoke.rs` worker probes) |
| A leading `~` in a double-quoted tmux argument expands to the home directory even for direct-exec argv. Encoding it as `\176` preserves the literal bytes. `quote` applies this encoding; the worker argv probe includes a literal `~` in shell and direct-argv replacements and their no-command repeats. Local result: tmux 3.7c; supported-version CI remains required. | `scripts/repro/worker-respawn.sh` |
| A pane-option `pane-died` hook with `remain-on-exit on` gives the pane a login shell in place when its process exits: same pane id, alive, in the given directory. The hook unsets itself and `remain-on-exit`, so the shell's own exit closes the pane and the session. `respawn-pane -k` does not fire the hook, and the hook survives it. The built-in launcher and muxa for tmux's `layout.sh` use this so a session stays after its agent exits. Keys typed into a login shell that is still starting can be lost while a hook resizes the pane, so neither types the agent. | `scripts/repro/shell-after-exit.sh` |

## The host daemon's client (ADR 0019)

| Fact | Script |
|---|---|
| `attach-session -f ignore-size,no-output` is accepted. With `no-output`, no `%output` arrives while a pane prints; without it, `%output` does. | `scripts/repro/daemon-client.sh` |
| An `ignore-size,no-output` client that stays attached next to a desktop client, and runs a command every 2 s, changes no layout. Detaching it changes none either. `make repro` keeps it attached 60 s; set `MUXA_REPRO_SECONDS` for longer. | `scripts/repro/daemon-client.sh` |
| A desktop client's key press updates its `#{client_activity}` (epoch seconds). `send-keys` from a control client does not update another client's `#{client_activity}`. `#{client_flags}` contains `control-mode` for a control client, and not for a desktop client. In `list-clients`, `#{session_id}` is the client's session id, so the daemon matches sessions by id. | `scripts/repro/client-activity.sh` |
| When its session is destroyed, the daemon's client is detached like any control client; the daemon attaches again for its next work. | `crates/muxa-bridge/tests/daemon.rs` `the_daemon_attaches_one_no_output_client_and_reattaches` |

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
