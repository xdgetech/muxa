# 0089: A talk may add the Inbox items the user asks for

## Status

Proposed (2026-11-24). Amends ADR 0083 decision 3. Wire: `docs/agents.md`, `.muxa/bot.json`; `protocol/PROTOCOL-v2.md`, "Talking to a Bot".

## Context

ADR 0083 let a talk write only its memory and a settings proposal; only a Routine run put items in the Inbox. In use, the developer asked a Bot in a talk to read a meeting note and make Inbox items for the actions. The Bot drafted them, then could only offer to propose a one-off Routine. The user was in the conversation and asked for exactly those items.

## Decision

1. **`items` in `.muxa/bot.json`**, up to 20, in the Routine output's item format (`kind`, `key`, `title`, `body`, `links`, `due`, and a proposal's `action`, `target`, `content`).
2. **Checked like a Routine's items**, before anything is written: one bad item refuses the whole output, memory and proposal too.
3. **They reach the Inbox at once**, as the Bot's items with the talk's `terminal`, keyed by the Workspace, the Bot and the item's `key`, so the same key never makes a second item.
4. **They do not count against `daily_items`.** The limit keeps a Bot's unattended runs from flooding the Inbox; the user asked for these.
5. Settings still change only through a proposal the user applies.
6. **A Bot can be renamed** (`bot-rename`, from Settings only, never from a talk): its table, the Routines and teams that name it, its folder with the Soul and Memory, and its Inbox items and counts move to the new name in one update. A typo in a name no longer means remove and add again, losing the memory.

## Consequences

- "Make items from this" works in a talk, on any client, with no Routine.
- A talk can add up to 20 items per output. The user sees them arrive and is in the conversation that made them.
- Alternatives not taken: a one-off Routine per request (the Bot's offer before this ADR): a settings change and a run to get a list the Bot already has.
