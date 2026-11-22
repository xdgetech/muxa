# 0077: A Task has one repo or none; a repo's own checkout can be a place

## Status

Proposed (2026-11-22). Amends ADR 0068 for the apps. Wire: `protocol/PROTOCOL-v2.md`, `start-task`.

## Context

ADR 0068 let a Task name many repos, each as a branch or read only. In use, the Start Task sheet with one picker per repo was harder than v1's choice of one repo or none. With no repo, the agent only had its folder: its permissions did not cover the Workspace's repos. v1 also had a session in a repo's main checkout (ADR 0031's ask session), which v2 lost.

## Decision

1. **The apps ask for one repo or none.**
   - One repo: a branch worktree (with `branch`) or read only (`read` beside the folder).
   - No repo: the Task's folder plus every repo of the Workspace read only, as one place kind `all-read`. The server expands it, so the apps need no repo list and the rule lives in one place.
2. **`checkout {repo}` makes the repo's own checkout the working place.** It is v1's ask session: Focus lists each repo with "start", which runs one agent in that checkout.
   - It reuses ADR 0076's adopt path: nothing is made, the busy check refuses a folder where another agent works, and Claude Code gets `--continue` when it has a conversation there.
   - The agent carries `muxa.checkout` with the repo as the request named it, so a second start returns the running agent. The checkout's path alone cannot match, because the request does not carry it.
   - Its place kind is `checkout`, never `branch`, so Approve never removes it (ADR 0043).
3. **The server keeps accepting every place list of ADR 0068.** Only the apps narrow the choice. `muxa-cli` keeps the full set.

## Consequences

- The Start Task sheets on iOS, the Mac and Android become one picker.
- "Read only" for all repos is a rule for the agent, not a boundary, as in ADR 0068.
- An agent in a main checkout changes files that the developer also uses. The Task's brief says so.
- Alternatives not taken:
  - No repo means the Workspace root, writable: the agent would edit every main checkout directly.
  - The apps send one `read` place per repo: three clients would repeat the Workspace's repo rule, and a long list goes on the wire.
  - Match a running checkout agent by path: the server would have to resolve the repo before it looks, which only the host can do.
