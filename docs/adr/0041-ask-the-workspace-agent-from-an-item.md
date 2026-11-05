# 0041: Ask the workspace's agent from an Inbox item

## Status

Proposed. It adds to ADR 0031 §4 (ask sessions) and ADR 0033 §4a (an item's answers). It raises the `work` level as ADR 0027 §5 says.

## Context

- A briefing offers ✓ and Later; a proposal offers Approve and Reject. Neither has a session, so Send has no agent to reach. The developer often wants one more step on an item: "put this in Notion", "draft a reply", "check the PR it names".
- Approve on a proposal makes one Task per action. For a small request a Task is heavy: a record, a folder, a row in Tasks.
- An ask session (ADR 0031 §4) runs the workspace's agent with no record. Today it runs in a repo's main checkout. A workspace's own accounts and MCP (`.muxa/mcp.json`) sit at its root, which is where a request about a briefing belongs.

## Decision

1. **Each workspace has one Ask session at its root.** It is an ask session (`@muxa_ask` is the workspace) that also has `@muxa_ask_root`. It runs the workspace's agent command and environment, like any ask session. Its name is `ask-<workspace>`, or `ask-<workspace>-<n>` when that name is taken. It lists under Sessions and closes like any ask session (ADR 0039).
2. **`ask-agent` answers an open item without a session with a text** (1 to 4096 bytes): a briefing, a proposal, a failed start. An item with a session has Send, to its own agent (ADR 0040 §4). The host types one prompt into the workspace's Ask session: the item's kind, source, question, summary and links, then the text. It starts the session when there is none, and types the prompt once the new agent reports `idle` (at most 30 s, as a Task's start prompt).
3. **The item stays open.** The request is not an answer. The developer closes the item with ✓, or keeps it.
4. **A busy agent still gets the text.** While the agent works, the text goes in as typed input; Claude Code takes it after the turn. The host refuses only when the agent waits on a menu (the text would pick an option), when its state is unknown, or when a person typed in the session in the last 5 s.
5. **The reply names the session.** `session-started {start, session, name}` comes before `inbox-saved`, for a new session and for the one already open. The apps open its Chat.
6. **`ask-agent` is in `answers[]`** of every open item without a session, before `later`. An item of a workspace the host does not know is `bad-request`. A client that does not know it shows nothing.
7. **`work` level 6** says the host does this.

## Consequences

- One conversation per workspace collects the requests. The agent keeps context between them, and the developer reads them in one Chat.
- The prompt carries the item's summary, so it reaches the agent's transcript. It never reaches a log (invariant 5).
- The Ask session runs on the workspace's default agent. A Routine's worker (its own tools and tokens) is not used, so a request may need tools the default agent lacks.
- Alternatives not taken:
  - A new Task per request: a record and a row in Tasks for each small request.
  - A new ask session per request: many sessions, and no shared context.
  - Waiting until the agent is idle: a long wait in the request, or a queue in the host, for what Claude Code already does.
