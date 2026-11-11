# 0055: A control client ends with its bridge

## Status

Accepted on 2026-11-11.

## Context

- On 2026-11-11 the developer's tmux server ran out of file descriptors (256 on macOS). About 70 `tmux -u -C attach-session -f ignore-size` processes had ppid 1 and stayed attached. Each one costs the server 3 fds.
- They were bridges' clients. `muxa-cli` drops its connection with `kill_on_drop`, so most commands end with SIGKILL to the bridge. The tmux child survives a SIGKILL.
- tmux lets a control client exit only after its output is written. When the bridge stopped reading while a pane printed and then died, that output never drains, and stdin EOF does not end the client (tmux 3.7c). Killing the client process ends it at once. See `docs/findings/tmux-control-mode.md`, "Control client lifetime".
- The tmux plugin's Inbox refresh runs `muxa-cli inbox --json` every 5 s per sidebar, so the leak grew by one client every few seconds.

## Decision

`muxa-tmux` starts every control client under a small `sh` guard, in its own process group:

1. `cat` forwards the bridge's commands to tmux.
2. When the bridge's end of that input closes (normal exit, crash or SIGKILL), tmux gets EOF and can detach as before.
3. 1 s later the guard kills its process group, so tmux is gone even when its output cannot drain.

The owner of the child process fixes it. Callers (`muxa-cli`, the plugin) need no cleanup step.

## Consequences

- Each control client has two more processes (`sh`, `cat`). Commands pass through one more pipe; `%output` does not.
- A client the bridge drops is detached within 1 s at the latest.
- `crates/muxa-bridge/tests/bridge.rs` `a_killed_bridge_leaves_no_control_client` and `scripts/repro/control-client-orphan.sh` check it.

## Alternatives not taken

- `muxa-cli` closes the bridge's input and waits instead of SIGKILL. That fixes one caller, not a bridge killed by anything else.
- Rely on stdin EOF: it is not enough on tmux 3.7 (the finding above).
- Require a tmux with an exit timer: tmux's master branch has one, but hosts run released tmux.
- A Linux parent-death signal: macOS has none, and it needs `unsafe`.
