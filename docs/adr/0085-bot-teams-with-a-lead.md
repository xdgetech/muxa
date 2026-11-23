# 0085: Bot teams with a lead, one level deep

## Status

Proposed (2026-11-23). Builds on ADR 0082. Takes its limits from ADR 0051 (removed in C3). Wire: BotSpec `members` and `handoffs_per_day` in `protocol/PROTOCOL-v2.md`; the output fields in `docs/agents.md`.

## Context

ADR 0082 planned teams after Bots proved useful: at least 3 enabled Bots whose items mostly end Done over two weeks. The developer asked to build them now and judge them in use. ADR 0051 showed the risk: agents that hand work to each other with no limit pile up runs and items.

## Decision

1. **A lead names its members.** `members` on a `[[bot]]` lists up to 6 other Bots of the same file. A member leads no team: teams are one level deep. `handoffs_per_day` (default 10) limits the team.
2. **Only the host hands work over.** A lead's run may add `handoffs: [{"to", "brief"}]` to `output.json`, one per member. After the output passes and publishes, the host starts each member as a terminal labelled `muxa.kind = handoff`, in a folder inside the lead run's folder. Nothing is typed into any terminal (invariant 2), and no approval key is pressed (invariant 7): a member's prompt shows in Needs you.
3. **A member starts from its own Soul and Memory** and the lead's brief, and writes `.muxa/result.json` with a summary and its new memory. The host keeps the summary and ends the member.
4. **The lead runs again with the results as data** once every member of the step has a result, or ended without one, or ran past an hour. The next run is the same Routine; its brief holds each brief and result.
5. **Limits.** At most 3 steps per occurrence: the last step may not hand off. Handoffs past `handoffs_per_day` are not started. Only the lead puts items in the Inbox, under its own daily limit.

## Consequences

- A team costs one run per member and one more lead run per step. The daily limit caps it.
- Members get their Workspace's connections and MCP servers like every agent there (ADR 0075), and may not edit `.muxa/`.
- A step's state lives in the lead run's folder (`.muxa/team/pending.json`); a restarted daemon goes on with it.
- Alternatives not taken:
  - Bots that message each other or share a chat: the step limit could not hold, and who publishes would be unclear.
  - A muxa MCP server for handoffs during a run: any same-user process reaches `server.sock`, so it needs its own ADR on tokens first.
  - Deeper teams: ADR 0051's runs grew fastest there.
