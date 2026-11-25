# Session backends: verified facts

ADR 0067 relies on these facts. Checked on a Mac on 2026-11-21: herdr-core 0.1.21, Rex 0.1.0-preview.21.

"Script" means `scripts/repro/herdr-core-api.sh` checks it. "Docs" means herdr-core's own documentation says it and nothing here checked it.

## herdr-core

| Fact | Source |
|---|---|
| herdr-core is Apache-2.0. Its API is JSON-RPC 2.0 over a Unix socket, or over stdin and stdout with `hc connect` (also `ssh host hc connect`). `hc schema` prints it as OpenRPC. | Docs (herdr.dev/core), `hc schema` |
| `hello` with `framed: true` switches to frames: `[channel u32 LE][kind u8][len u32 LE][payload]`. Channel 0 carries JSON-RPC. `hello` lists the features `attach` and `events`. | Script (DATA, CREDIT, CLOSE, RESIZE used) |
| The other frame kinds are FOCUS, FACTS and AUTOMATIC_INPUT. | Docs (`schema/README.md`) |
| `terminal.run` takes argv, `cwd`, a `name` and labels, and returns once the program runs. | Script |
| `terminal.run` refuses a `name` that a live terminal already has: JSON-RPC error -32000, `a live terminal is already named "…"`. muxa gives a Task's second and third shells `-sh2` and `-sh3`. | Seen on the developer's host (2026-11-25); the backend contract checks it |
| `terminal.attach` with `cols` and `rows` opens a stream. It starts with a first paint that holds the current screen and the latest OSC 7501 report. The program's raw output follows. | Script |
| The first paint also holds the history asked for and the title. | Docs |
| Bytes sent as DATA on the stream reach the program. | Script |
| The size given to `attach` is the program's size. A RESIZE frame changes it, and the program sees the new size. | Script |
| "The last active viewer sets the size"; an `observe` viewer never does. | Docs |
| A stream starts with 256 KiB of credit. The viewer returns CREDIT for the bytes it consumed. A viewer more than 4 MiB behind gets a fresh paint instead of the backlog. | Docs |
| A program's OSC 7501 report (`ESC ] 7501 ; state=done:app=x ESC \`) shows in `terminal.get` as `status`. | Script |
| herdr-core never infers a state from the screen. | Docs |
| Claude Code (2.1.330) sends OSC 7501 by itself, with no hooks: herdr-core shows `reported: true` and `status: {app: "claude-code", state: "working"}` for a running session, and `done` after a turn. Its `msg` is the current activity text, so it counts as conversation content (invariant 5). | Seen by hand on running sessions |
| `events.subscribe` with a label filter sends events on their own channel. The first is `created`; `status` and `size` follow. | Script |
| The full event list: `created`, `lost`, `restarted`, `keep`, `finished`, `removed`, `status`, `turn`, `title`, `labels`, `size`, `doc`, with a `heartbeat` every 15 s and `missed` plus a snapshot for a subscriber that fell behind. | Docs |
| `terminal.kill` ends the program, and the stream closes with `{"reason":"killed"}`. `terminal.rm` then removes it. | Script, `make test-herdr` |
| herdr-core has no panes or splits. The Herdr app groups terminals by the labels `herdr.space` and `herdr.tab`. | Docs (Herdr app) |

Not checked: programs surviving `hc server restart` (the docs say "programs keep running"), what survives a reboot, two viewers changing the size in turn, and whether Codex sends OSC 7501 by itself.

The script needs a running herdr-core and uses the user's own: herdr-core has no option for an isolated server that the script could start. It adds one labelled terminal and removes it. Run it by hand with `MUXA_REPRO_HERDR=1`; CI skips it.

## Rex

Checked by hand with the CLI bundled in the macOS app.

| Fact |
|---|
| `rex new <label> --cwd <dir> --keep-open --json -- <cmd>` returns the session id and its block ids. `rex capture -s <session> --trim` prints the screen. `rex block call <method>` reaches each terminal's methods: `process` (pid, cwd, last exit), `size` (size, owner, each client's wanted size), `program_status`, `write`, `resize`, `list_dir`, `download_begin`. |
| `rex api list` documents about 90 control methods, with input and output schemas from `rex api describe`. |
| The live output stream is `data.connect`, a one-time ticket for the `rex.data.terminal.v1` subprotocol. Its frame format is not documented, and `rex block stream` answers `command "rex block stream" is not implemented`. |
| A terminal's size applies when the caller owns the block or no client does (from `rex api describe`). |
| `client.key` gives the server's Lua key bindings the first chance at a key (from `rex api describe`). |
| Built-in Tailscale is off by default. With `allow_tailnet_members: false`, only the Tailscale user who signed the server in may connect (from `rex api describe`). |
| Rex documents the same OSC 7501 format. A report in a wrong format was ignored; a correct one was not tried. |
| Rex lists clients for macOS, iOS (testers only) and the web. There is no Android client. |

## tmux

ADR 0067 replaces tmux. Its facts stay in `docs/findings/tmux-control-mode.md` until the tmux code is removed.
