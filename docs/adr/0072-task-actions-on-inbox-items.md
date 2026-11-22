# 0072: Approve, Request changes and Retry act on what made the item

## Status

Proposed (2026-11-22). Builds on ADR 0043, 0066 and 0068. Wire: `protocol/PROTOCOL-v2.md`, Task actions.

## Context

The Inbox (ADR 0066) shows a Review item with Approve and Request changes, and a Failed item with Retry (`docs/next.md`). Until now the server did not know which Task or Routine made an item, so these actions could only mark it Done.

## Decision

1. **An item links what made it.** `InboxItem` has `terminal` (the Task's agent terminal) or `routine` (`{workspace, name}`). The server sets them when it makes the item. Items stored before have neither and still load.
2. **Approve closes the Task.** It runs ADR 0043 §2 for each branch worktree on the agent's `muxa.places`, and then ends the agent, removes the worktrees and branches, and marks the item Done.
   - The label is the list of worktrees muxa made (ADR 0043 §5) for a v2 Task. No separate list file.
   - Merged means the branch tip is in the default base (`origin/HEAD`, else the main checkout's branch) or in the main checkout's branch. muxa does not ask `gh` about the PR. A squash-merged PR therefore reads as not merged, and the user approves with `force`.
   - When `git branch -d` refuses a branch that muxa's rule calls merged (it is in `origin/main` but not the local `main`), muxa deletes it with `-D`.
   - The check runs before anything changes. A worktree that changes in the moment between the check and the removal is not checked again.
3. **Request changes types the comment as the user's input.** This is the user's own text, so invariant 2 allows it. One line is typed with Enter, as the Task Loop types its prompt. Several lines go as one bracketed paste, then Enter. Without the paste, the agent would send each line on its own. The server does not ask whether the program turned bracketed paste on. Claude Code and Codex are expected to; no probe in `docs/findings/` confirms it yet, and one should before the apps offer multi-line comments.
4. **Retry decides by the item's link and the agent's state.**

   | Link | Agent | Retry |
   |---|---|---|
   | `routine` | - | runs the Routine now |
   | `terminal` | live | starts its Task Loop again with the same check, failed count 0 |
   | `terminal` | ended | starts the Workspace's agent again in the same working place |

   A live agent with a Failed item can only have failed through its loop, so the state tells the two Task cases apart without a stored kind.
5. **A restarted agent has the same places, labels and connections.** `start-task` now records the named connections in a `muxa.connections` label (names only) so a restart can give them again. A restart is not in plan mode: the plan was the start's choice. It does not continue the old conversation.

## Consequences

- The apps can offer the actions of `docs/next.md` with one request each. The UI is a separate step.
- An agent started before `muxa.connections` restarts with the default connections only.
- A Task whose agent terminal was removed outside muxa cannot be closed by Approve. muxa no longer knows its places, so it says so and the user marks the item Done.
- Alternatives not taken:
  - Store each item's kind of failure (`loop` or `agent`) on the wire: one more field for what the agent's state already says.
  - Keep a worktree list file as ADR 0043 §5 describes: a v2 Task already carries its places on its terminal.
  - Always paste, also one line: a program without bracketed paste would get the escape bytes as keys.
