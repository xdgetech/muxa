# 0039: `close-task` closes ask sessions

## Status

Proposed. It changes ADR 0031 §4, "Closing it is closing any manual session". It raises the `work` level as ADR 0027 §5 says.

## Context

- An ask session (ADR 0031 §4) runs in a repo's main checkout. It has `@muxa_ask`, no worktree and no record.
- `close-task` closes only a session with a worktree. The apps close no other session, so an ask session closes only in tmux (muxa for tmux's `x`).
- Invariant 2: the bridge never closes the user's own sessions. A session that muxa did not start must stay out of reach.

## Decision

1. **`close-task` also closes an ask session.** It qualifies only when it has `@muxa_ask`, no `@agent_worktree` and no `@muxa_task`. The session is found by id. Any other session without a worktree stays `bad-request`.
2. **It closes as a task's session does.** Its device sessions close first, and attached clients move to the most recently used other session. `mode` changes nothing: the main checkout and its files are never touched.
3. **A session carries `ask`**, the workspace from `@muxa_ask`, in `list`. Clients show Close Session only on a session with `ask`.
4. **`work` level 5** says the host does this. An older host refuses the request and sends no `ask`, so the apps hide the action.
5. **The CLI has `muxa-cli ask close SESSION`**, by id or name. It sends `close-task` with the id.

## Consequences

- An ask session closes from both apps and from the CLI, as it does in tmux.
- Alternatives not taken:
  - A new `close-session` request: a second close path, with the same client moves and device-session rule.
  - Closing any session without a worktree: it would close the user's own sessions (invariant 2).
  - A capability `close-ask`: ADR 0027 §5 asks for one level instead of a capability per feature.
