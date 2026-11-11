# 0054: Talk to the secretary: one conversation with the overall assistant

## Status

Accepted on 2026-11-11. It amends ADR 0041 (any workspace's root Ask session opens without an Inbox item) and ADR 0049 §2 and "Alternatives not taken" (the overall assistant gets a resident conversation). It amends the non-goal "無制限の常駐 agent" of the [specification](../personal-assistant.md) §12 for the overall workspace only. Milestone S0 is implemented; hosts advertise `work` level 15.

## Context

- The Personal Assistant has stores (Captures, Work items, requests, plans, policies) and a morning plan (ADR 0048–0053). The user reaches it through forms, the Inbox and Today. There is no place to just talk to it, as with Dot or Meta Muse.
- The overall assistant is a workspace with `[workspace.assistant] overall = true` (ADR 0049 §2). Its agent runs only as hidden plan runs. ADR 0049 rejected a long-lived session: it needs a restart path and holds context nobody reviewed.
- Every workspace already has one Ask session at its root (ADR 0041). Today only `ask-agent` on an Inbox item opens it, and clients leave the overall workspace out of their pickers.
- The spec keeps the stores as the source of truth: the conversation is the entry, not the record (`personal-assistant.md` §9).

## Decision

### 1. One resident conversation, for the overall workspace only

The secretary's chat is the overall workspace's root Ask session, `ask-<workspace>`. It may stay open while the user is away. No other workspace gets a resident agent. Closing it after it is idle, and resuming the conversation on the next message, is a later milestone (§7).

### 2. What the secretary does without approval

- Save Captures.
- Make, sort and merge Work items, and move them down with Later.
- Send requests to workspaces with `overall_requests = "auto"`, within every budget of ADR 0048 §8.

Approval stays for: starting Tasks, writes outside the host (ADR 0052), requests to `review` workspaces, and policies, which are the user's own words (ADR 0050).

### 3. Results reach the user where they do today

The Inbox and its notifications show results. The chat does not replace them.

### 4. `share` is a rule in the brief, until the CLI enforces it

The secretary uses `muxa-cli`, which can read every workspace. That bypasses `share` (ADR 0049 §3). Until a later milestone adds a CLI read filter for the overall agent, the secretary's brief tells it not to read or repeat more than each workspace's `share` allows.

### 5. `sort` proposal items go away later

The secretary sorts Captures directly (§2). The `sort` items of ADR 0049 §4 will be removed in a later milestone.

### 6. S0: what this ADR builds

- **Host.** `start-session {workspace, root: true}`, without `repo`, opens the workspace's root Ask session: the open one, or a new one, as `ask-agent` does. The reply is `session-started`, then `result`. Any workspace works, the overall one too. `repo` with `root`, or neither, is `bad-request`. `work` level 15 says the host does this. `muxa-cli ask --root --workspace W` sends it. New Task and Capture pickers still leave the overall workspace out.
- **Brief.** `docs/examples/secretary-CLAUDE.md` holds §2–§4 and the `muxa-cli` commands. The user installs it as `CLAUDE.md` at the overall workspace's root. Claude Code also reads it in plan run folders below the root, so its first lines tell a run to stop there.
- **Clients.** Talk to Assistant, in the `+` sheet and the Today header of iOS and Android, sends `start-session` with `root` for the overall workspace and opens the session's Chat. It shows only when the host has an overall workspace and level 15. In tmux, `prefix + A` offers the overall workspace's root session.

### 7. Not in S0

- **Resume.** muxa starts the root Ask session with the plain agent command. A new session starts a new conversation. `claude --continue` exits when the folder has no conversation, and it picks the folder's newest one, which may be a manual session (`docs/findings/claude-code-resume.md`). The next milestone keeps the Claude Code session id of the secretary's session and starts the next one with `--resume <id>`.
- **Who is asking.** The host cannot tell the secretary's `muxa-cli` calls from the user's. A request to a `review` workspace is refused only by the brief. The CLI read filter (§4) and an actor mark on the secretary's calls come together.
- **Idle close**, and removing `sort` items (§5).

## Consequences

- The user has one place to talk, and every decision still lands in a store that the apps, tmux and the plan read.
- The secretary's limits in S0 are the brief's words, not the host's checks. A prompt injection in a Capture could make it send a `review` request or read a workspace it should not. The budgets still hold. The follow-up milestones move these limits into the host.
- Alternatives not taken:
  - A new chat screen with its own message store: a second record of what the user said. The Ask session's Chat already shows the agent's transcript.
  - Opening the root session with `start-session {repo: ""}`: an empty string that means something else than the field says.
  - A resident agent per workspace: many sessions, and context nobody reviewed, as ADR 0049 said.
