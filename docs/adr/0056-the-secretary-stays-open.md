# 0056: The secretary stays open

## Status

Accepted on 2026-11-11. It amends ADR 0054 §1 and §7: "close after idle" is dropped, and resume is built. It amends ADR 0019 (the daemon's tmux client stays attached while the secretary is resident). No protocol change; hosts still advertise `work` level 15. Amended by ADR 0061: Retry on the give-up item resets the restart cap.

## Context

- The secretary's chat is the overall workspace's root Ask session, `ask-<workspace>` (ADR 0054). It opens only when a client asks, through `start-session {workspace, root: true}` or `muxa-cli ask --root`. When its agent exits, the pane gets a login shell; when the session closes, it stays closed.
- The user asked for the secretary to be resident in tmux too: always there, not a session to open first. Closing it after it is idle, ADR 0054's plan, is the opposite.
- A restarted session starts a new conversation. ADR 0054 §7 planned to keep the Claude Code session id and start with `--resume <id>`.
- Claude Code facts (`docs/findings/claude-code-resume.md`): `--resume <unknown id>` exits at once with status 1. `--resume <known id>` stays open, shows the conversation, and writes the same id to its `sessions/<pid>.json`.

## Decision

### 1. The daemon keeps the secretary's session open

- The daemon looks every 10 s. When `[assistant] enabled` is true and the overall workspace sets `resident` (default true), it keeps that workspace's root Ask session open:
  - No session with `@muxa_ask <workspace>` and `@muxa_ask_root`: it opens one through `ask::start_root`, so the naming rules stay: `ask-<workspace>`, or `ask-<workspace>-2` when the user has a session of that name.
  - The session's agent pane (`@muxa_role agent`) is dead: it respawns the pane in place (`respawn-pane -k`). The pane id, size and layout stay, and clients on the session stay.
  - A session without an agent pane is left alone: the user closed that pane.
- The secretary's session gets `@muxa_secretary 1`. Its agent pane has `remain-on-exit on` and no shell after exit (`scripts/repro/dead-pane-respawn.sh`). A session that closed would detach the user's client from tmux (`detach-on-destroy`); a dead pane does not.
- `[workspace.assistant] resident = false` turns it off. The session then opens only when a client opens it, as in ADR 0054, with a shell after the agent exits.
- The daemon never starts a tmux server. Without one, it does nothing and looks again after 60 s.
- Invariant 2 holds: the daemon creates and respawns only its own session and pane. It selects nothing, resizes nothing and changes no user session's options.

### 2. A cap on restarts

- The first start is at once. A restart waits 5 s, then 30 s, then 120 s after the start before it.
- At most 3 restarts in 10 minutes. Then the daemon stops, and makes one `failure` Inbox item in the overall workspace: "The assistant's chat exited 4 times in 10 minutes…". It keeps the dead pane, so the user can read why. The daemon starts it again once its agent runs again, or after a daemon restart: the user closes the chat's session and opens it again, or runs `muxa-bridge daemon restart`.
- The counts live in the daemon's memory. A daemon restart starts them over.

### 3. The same conversation after a restart

- While the agent runs, the daemon reads its Claude Code session id every 30 s through the adapter (`AgentAdapter::find`), only when the adapter ties the session file to the pane's process (`process_bound`). It keeps the id in `<state dir>/assistant/secretary.json`, mode 0600. Ids and times only.
- A start, by the daemon or by a client, appends `--resume <id>` to the workspace's agent command when muxa has an id and the command starts `claude` (`secretary/command.rs`). A command with shell syntax, another program, or its own `--resume`/`--continue`/`--session-id` starts fresh.
- A resumed start that ends within 30 s did not find its conversation. The daemon drops the id and writes `resumeFailedAt`. The next start is fresh, once; the id of the new conversation is kept as before.
- `/clear` in the chat starts a new conversation with a new id; muxa keeps the new one. That is how the user drops the context.
- Codex: muxa keeps no id, and the agent starts a new conversation each time. Codex threads have their own resume (ADR 0028), not used here.

### 4. tmux

muxa for tmux's tree lists the secretary's session first in Ask, marked `assistant` (`@muxa_secretary`). No new key: `prefix + A` already offers it, and the tree's jump keys reach it.

## Consequences

- The secretary is always one jump away, in tmux and in the apps, and keeps its conversation across agent exits, session closes, daemon restarts and host reboots.
- One agent runs at all times on the host while the daemon runs. It costs nothing between turns, but it holds a Claude Code process and its memory.
- The daemon's tmux client stays attached while the secretary is resident, because it looks every 10 s. A plugin's `client-attached` hook runs less often, not more.
- A bad agent command makes 4 starts in about 3 minutes, then one Inbox item.
- Without a running daemon, nothing restarts the agent: its pane stays dead until the user closes the session. The overall assistant's plans need the daemon anyway (ADR 0048 §8).
- The secretary's limits are still its brief's words (ADR 0054 §4). A resident agent does not change what it can reach; it is reachable more of the time.
- Alternatives not taken:
  - Close after idle, then resume on the next message (ADR 0054 §7): the user asked for the opposite, and a closed session needs a client to open it again.
  - `claude --continue`: it exits in a root without a conversation, and picks the newest conversation in the folder, which may be the user's own.
  - A shell after the agent exits, as other sessions have: the daemon would have to tell a user's shell from an exited agent, and a closing session detaches the user.
  - A new key, `prefix + S`: a second way to the same row; the tree and `prefix + A` are enough.
