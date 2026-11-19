# 0064: Match a Claude Code instance that recorded no pane

## Status

Accepted on 2026-11-19. It amends ADR 0005 "Matching an agent to a pane uses recorded facts" for Claude Code.

## Context

- ADR 0005 matches a Claude Code instance to a pane by the `tmux` value of its `sessions/<pid>.json`: the pane id and the session name.
- Some instances leave `tmux` out, or write it empty (`":."`), though they run in a tmux pane with `TMUX_PANE` set. Seen with Claude Code 2.1.319 and 2.1.327 next to instances of the same version that record it. The cause is inside Claude Code.
- Such a pane shows no Chat, no agent state and no approval card. Its transcript exists.

## Decision

When `tmux` names no pane, the instance matches a pane when both hold:

1. Its pid runs inside the pane, checked with `ps`, as for every instance.
2. Its `cwd` is the pane's path.

A `tmux` value that names a pane still decides alone, as before. It is never overridden by the directory.

## Consequences

- A pid inside the pane's process tree belongs to that pane, on whatever server, so the reason for checking the session name (pane ids restart with the server) does not apply.
- An instance that changed directory after it started, without a recorded pane, still shows the terminal only.
- The same lookup serves chat, states (ADR 0008), plan documents and the run observation of ADR 0024.

## Alternatives not taken

- **Process tree only, without the directory.** Enough to find the pane, but the directory keeps a second recorded fact in the match, as ADR 0005 asks.
- **Read `TMUX_PANE` from the process environment** (`ps -E`, `/proc/<pid>/environ`). It is an inherited value that a parent can set wrong (ADR 0016 refuses it in workspace environments for that reason). `ps -E` prints the environment space-separated, so values cannot be split reliably, and it needs one more `ps` per instance.
