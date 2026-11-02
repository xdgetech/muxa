# 0021: The device sizes its window from a device session

## Status

Proposed. It amends ADR 0020: the rule, the protocol and the pane fit stay; how the bridge becomes the window's latest client changes. It allows `link-window`, `kill-window` and `set-option`, but only on the bridge's own device session, and `close-task` kills those sessions before the task's session.

## Context

- ADR 0020 sizes a window through the bridge's control client, by making it the latest client with `switch-client -t <session>`. That makes it latest only for the session's *current* window. When the watched pane is in another window, the window keeps the desktop's size and the device scales the pane down. `aggressive-resize on` has the same effect. The developer runs `window-size latest` with `aggressive-resize on`, so the limit hits every pane that is not in the laptop's current window.
- The goal: the laptop keeps its own sizes and settings; on a phone or tablet, every pane is shown full screen at its best fit, in any window.
- The bridge may not change the user's current window or pane: the laptop would jump to another window.
- The facts, each with a repro script (`docs/findings/tmux-control-mode.md`, "Sizing from a device session"):
  1. A session of its own can hold one window of the user's session (`new-session -d`, `link-window`, `kill-window` of its first window). The user's session keeps its windows, its current window and its clients' view.
  2. A sized control client in that session sizes that window only, also when it is not the desktop's current window, with `aggressive-resize` off and on. A desktop key press in that window takes it back; the device client's `switch-client` takes it again.
  3. A grouped session (`new-session -t`) shares every window. With `aggressive-resize off` its client sizes every window whose latest client is gone, also windows the device does not show.
  4. A command with no target, such as `tmux attach` on the laptop, picks the session switched to last. That is the device session right after the device's `switch-client`, until another client switches to the user's session.
  5. Killing the user's session leaves the linked window alive in the device session.
  6. `list-sessions`, `list-windows -a` and `list-panes -a` show the device session. A session option hides it from a `-f` filter.
  7. `destroy-unattached` crashed tmux 3.7c when the session ended while its client was still exiting.
  8. tmux-agents' hooks run for the device session and change no pane.

## Decision

### One device session per watched window

When a client with a `viewport` watches a pane in a `window-size latest` window, the bridge sizes that window through a **device session** and a **device client**:

1. The bridge's own control client (the main client) runs `new-session -d -P -F '#{session_id} #{window_id}' -s _muxa-<pid>-<n> cat`. The `cat` window is a placeholder.
2. It runs, by id: `set-option -t $G @muxa_device 1 ; set-option -t $G @muxa_device_pid <pid> ; set-option -t $G detach-on-destroy on ; link-window -d -s @W -t $G: ; kill-window -t <placeholder>`. The session now holds only `@W` (fact 1). When the session ends, its client detaches, even under the user's `detach-on-destroy off`; otherwise the sized client would move to a user session and size its window.
3. It attaches a second control client to `$G` with `-f ignore-size,no-output`. It never gets pane bytes; the main client streams them as before.
4. The device client takes the window as in ADR 0020: `refresh-client -f '!ignore-size' ; switch-client -E -t $G ; refresh-client -C WxH`.
5. The main client then sends `switch-client -E -t $S`, to the session it is already on. That makes `$S`, not `$G`, the session a command with no target picks (fact 4).

Typing, a new viewport and the pane fit of ADR 0020 send their `switch-client` and `refresh-client -C` on the device client, followed by step 5.

### When it ends

- **A watch of a pane in another window or session, an unwatch, a pane gone, or a window that is no longer `latest`:** the device client sends `refresh-client -f ignore-size ; refresh-client -C WxH`, and the main client kills `$G` with the device client still attached. A new window gets a new device session.
- **The client leaves** (end of input, `bye`, idle timeout) **or the bridge gets `SIGTERM`:** the same, before the bridge exits.
- **A person joined the device session** (`choose-tree`): the bridge detaches its client and leaves the session.
- **The bridge crashes** (`kill -9`): its tmux clients exit with their stdin. The device session stays, unattached. The window keeps the device's size until a desktop client views it, or, with `aggressive-resize off`, until the next size change.
- **Leftovers:** when a bridge starts, and before it opens a device session, it kills device sessions that have no client and whose `@muxa_device_pid` is not a running process.
- **The user's session closes:** muxa's `close-task` kills the device sessions that hold its windows first, then the session (fact 5). tmux-agents' `close.sh` does the same. When the session is gone some other way, the bridge sees `%sessions-changed`, finds the pane gone and closes its device session.

### Hidden from lists

- Every session list in muxa skips sessions with `@muxa_device`: `list`, the Task views, the fallback that links a session to a Task by its directory, the daemon, and the "most recent other session" that `close-task` moves clients to.
- tmux-agents skips them in its tree, status line, keys and sidebars, with the filter `#{==:#{@muxa_device},}` (its README, "For other tools"; muxa's `docs/agents.md`).
- A bridge or daemon whose client lands on a device session at attach moves to the most recent other session.

### What stays protected

- Invariant 1 is unchanged.
- The user's sessions and every desktop client keep their current window and pane. No command selects a window in a user's session.
- The bridge sets options only on its own device session: `@muxa_device`, `@muxa_device_pid` and `detach-on-destroy`. It never changes `window-size`, `aggressive-resize` or any option of the user's sessions. It does not use `destroy-unattached` (fact 7).
- It never sends `resize-pane`, `resize-window`, `select-window`, `select-pane` or `refresh-client -A '%p:off'`.
- The daemon's client and clients without `viewport` are unchanged: no device session, no size.
- Logs carry session, window and pane ids and sizes only.

### Limits (accepted)

- Two devices on one window: the latest one gets its size, as in ADR 0020.
- `choose-tree` and tmux's own `switch-client -n`/`-p` show the device session while a device watches. Choosing it shows the watched window alone.
- With `aggressive-resize on`, a window the device leaves keeps the device's size until a desktop client views it.
- A desktop key press takes the window back only in that window, as for any terminal under `window-size latest`.
- A user's `session-created` hook runs once per device session, with the placeholder window.

## Consequences

- Every pane, in any window, fills the device at its default font while the device is in use. The laptop keeps the size of the window it shows.
- Each watched window costs one more tmux client process, one short-lived session, and a placeholder `cat` that ends at once.
- `session-created`, `window-linked`, `window-unlinked` and `session-closed` hooks, and `%sessions-changed`, fire once per watched window.
- Tools that list sessions, windows or panes must skip `@muxa_device`. muxa and tmux-agents do; other plugins show the window twice.

Rejected:
- **A grouped session** (`new-session -t`) with `select-window` in it. It holds every window, so with `aggressive-resize off` its client sizes windows the device does not show (fact 3).
- **Zoom (`resize-pane -Z`).** It changes the user's layout: the laptop would see the zoomed pane too.
- **Per-window sizes** (`refresh-client -C @id:WxH`). tmux 3.2a refuses them, and they pin the window in every mode, so the laptop cannot take it back (ADR 0020, fact 7).
- **`select-window` in the user's session.** The laptop would jump to the watched window.
- **One device client for every window.** A client stays latest of each window it was on.
- **The main client in the device session.** Its `%output` would cover only one window, and the stream code would need a second session to follow.
- **`destroy-unattached` for cleanup.** It crashed tmux 3.7c (fact 7); a crash would end every session of the user.

## Implementation plan

| Step | What | Verified by |
|---|---|---|
| 1 | Repro scripts and findings rows | `make repro` on 3.2a, 3.4 and the latest |
| 2 | This ADR, AGENTS.md invariant 2 | review |
| 3 | The bridge's device session; hiding it from lists; cleanup; tmux-agents skips it | `make check`, `make test-tmux` with a pty desktop client: a pane in a non-current window takes the device size and the desktop's current window and pane stay; a desktop key press takes it back; no leftover after disconnect or `kill -9` and the next start; `list` never shows it; `aggressive-resize` off and on; tmux-agents' bats tests |
| 4 | README, `docs/plan.md`, `docs/agents.md` | review |
