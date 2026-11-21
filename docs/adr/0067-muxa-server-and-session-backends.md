# 0067: A muxa server owns the terminals through a session backend

## Status

Proposed (2026-11-21). Facts: `docs/findings/session-backends.md`.

Changes: ADR 0003 (the bridge becomes a relay; the transport stays), ADR 0019 (the daemon becomes required; the bridge no longer serves requests itself, so tests stop running each case twice). Supersedes, once the switch is done: ADR 0001, 0020, 0021 and 0055 (tmux control mode and its sizing).

## Context

Today each app connection runs its own `muxa-bridge` over SSH exec (ADR 0003). The bridge attaches to tmux in control mode, polls agent states every 2 s, and builds the Inbox from disk on each request. The daemon (ADR 0019) is optional and does the same polling while no client is connected.

This has four costs:

1. A reconnect starts a new process, a new control client and a fresh snapshot. The app drops its connection on every network change or return from the background, and there is no session to resume.
2. Polling runs once per bridge plus once in the daemon.
3. Terminal output goes one `%output` at a time, base64 in a JSON line, with no batching (`crates/muxa-bridge/src/watch.rs`).
4. Window sizing through tmux needs device sessions and `link-window` (ADR 0020, 0021).

herdr-core (Apache-2.0, preview 0.x) has a documented JSON-RPC API with a raw output stream, flow control, events and OSC 7501 status. Rex has a broad documented control API, but its output stream is not documented.

## Decision

1. **The daemon becomes the muxa server, and it is required.** It is one process per user, run as a LaunchAgent in the user's GUI domain on macOS. It owns:
   - the backend connection;
   - agent states, polled once;
   - the Inbox store;
   - Routines;
   - the scheduler (slots and resource checks);
   - one session per client that the client can resume.
2. **The bridge becomes a relay.** The app still runs `muxa-bridge` over SSH exec, so SSH keeps doing authentication. The bridge connects stdin and stdout to the server's Unix socket (mode 0600, peer uid checked). No port is opened.
3. **A client can resume.**
   - The server gives each client a token. It keeps the token on disk; it keeps buffers in memory only (invariant 5).
   - Pane output carries a sequence number per pane. The server keeps 256 KiB of recent output per watched pane.
   - Input carries a client sequence number. The server acknowledges it, and ignores a number it has already applied, so an approval is never typed twice.
   - The Inbox carries a revision.
   - On reconnect the client sends its token, its last sequence numbers and its last revision. The server sends what is new.
   - A fresh snapshot is sent when the buffer no longer covers the gap, after a resize, after the backend sends a fresh paint, or when the client lost its screen state (the app was killed).
   - A token expires after 24 hours without use.
4. **Terminals sit behind a session backend trait.** The trait works on one terminal at a time:
   - list, run (argv, cwd, name, labels, env);
   - attach: a stream of the first paint and the raw output, with input, resize and close;
   - read (a screen and history as text);
   - kill, events, labels.

   Workspace, Task and place stay in muxa's records; the backend only stores labels.
5. **herdr-core replaces tmux.** It is the only backend, over `hc connect` in framed mode. The trait exists so that Rex can replace it later, when Rex's terminal stream is documented. It is not for running two backends side by side.
   - muxa pins the herdr-core version it was tested with, and the server checks the protocol generation at start. herdr-core is Apache-2.0, so a breaking release can be skipped or forked.
   - **The switch:** tmux sessions that are open at the switch finish in tmux with the current apps. New Tasks start in herdr-core. Then `muxa-tmux`, the control-mode code and its tests are removed.
6. **The server attaches once per client.** With herdr-core, each client that watches a terminal gets its own viewer, so "the last active viewer sets the size" means the device in use.
7. **The apps own pane layout.** herdr-core has no splits. The apps draw a Task's panes and keep the layout per device. The wire protocol carries raw bytes, sizes and states, and never a backend's own types.
8. **State comes from OSC 7501 where a program sends it.** The agent adapters (ADR 0005) stay for questions, transcripts and Chat. Under herdr-core an adapter matches an agent to its terminal by `HC_TERMINAL_ID` in the agent's environment, the way it uses `TMUX_PANE` today. The rule "no invented state" (invariant 6) holds.
9. **A contract test suite checks the backend.** It checks: run, attach, input round trip, resize reaches the program, status from OSC 7501, kill with a reason, and events in order. A Rex adapter must pass the same suite.

## Consequences

- One poller and one cache. Inbox and state updates become deltas.
- Reconnects are fast and mostly invisible. The app shows its last state at once and catches up.
- Output can be batched and sent as binary frames. herdr-core's credit gives backpressure; past 4 MiB behind it sends a fresh paint, as muxa's resnapshot does today.
- AGENTS.md invariants 1–4 describe tmux. When this ADR is accepted, they are rewritten for herdr-core: never touch terminals muxa did not start or was not asked to watch; the size follows the device in use through its own viewer; target terminals by id; every value reaches herdr-core as JSON, never as a shell line. `make test-tmux`, `make repro`'s tmux scripts and `docs/findings/tmux-control-mode.md` are retired with the tmux code.
- The user's desktop terminals move from tmux to herdr-core. The developer already runs most terminals there.
- The server is a single point of failure on the host. It must restart without losing client tokens or Routine schedules. herdr-core's docs say programs keep running across its own restart; that is not checked yet.
- Desktop keys follow VS Code (`docs/next.md`) in the apps (ADR 0070).

## Alternatives not taken

- **Stay on tmux and only add the server.** It keeps the control-mode parsing and the sizing work, and the desktop UI stays tmux's.
- **Keep a tmux adapter beside herdr-core.** Two backends double the tests and the invariants for no user gain.
- **Rex first.** Its control API is broad and was checked on the host, but `rex block stream` answers "not implemented" and `data.connect`'s subprotocol is not documented. Building on it now means reading an undocumented preview protocol. Rex has no Android client.
- **muxa owns the PTYs.** It would make muxa a terminal multiplexer, with its own persistence, emulator and desktop UI. That is the scope that made muxa costly.

## Not decided

Check before building:

1. Do Claude Code and Codex send OSC 7501 by themselves? If not, which hooks report it, and is Needs you reliable without them?
2. Do programs survive `hc server restart` and a reboot?
3. What happens when two viewers resize the same terminal in turn?

The server is required on Linux hosts too (decided 2026-11-21). A host without systemd user sessions starts it some other way, for example from the shell's login.
