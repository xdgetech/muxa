# 0083: Talk to a Bot to set it up; it proposes settings the user applies

## Status

Proposed (2026-11-23). Builds on ADR 0082 (Bots), 0068 (Task places), 0074 (Settings in the apps) and 0075 (connections per Workspace). Wire: `protocol/PROTOCOL-v2.md`, "Talking to a Bot".

## Context

ADR 0082 made a Bot a set of fields, a Soul and Routines, which the user edits in Settings. Writing a good Soul and good Routine instructions by hand is the hard part. Orca bots and Grok Bot start with a conversation, and the developer wants that. A conversation must not become a way around the user's control of settings, and muxa already has a conversation surface: a Task with Chat and the Terminal.

## Decision

1. **A talk is a Task.** `start-task` takes `bot`: a folder Task in the Workspace whose agent gets `muxa.bot`. Chat, the Terminal, restart and Close work as for any Task. A talk is not resident. It ends when the user closes it; the next talk starts from the same files.
2. **The talk starts as the Bot.** `.muxa/TASK.md` holds the Bot's `soul.md`, its `memory.md`, its settings and its Routines with their instructions, the other Bots of the Workspace, and how to propose. A name with no Bot yet starts a talk that may propose adding it.
3. **The Bot proposes; it never edits settings.** Claude Code may not edit the Workspace's `.muxa/` in a talk (the same deny rule as a Bot's runs). The agent writes `.muxa/bot.json` with `memory` and a `proposal` of settings ops, then ends its turn.
4. **The host checks each new `bot.json` when the agent reports `done`.**
   - Allowed ops: `bot-add` and `bot-update` of this Bot, and `routine-add`, `routine-update` and `routine-remove` of this Bot's Routines, in this Workspace. Anything else refuses the whole output.
   - A Routine it adds is added turned off.
   - The ops must pass the loading rules now (a dry run of `settings-update` that writes nothing).
   - The memory is written only when the whole output passes.
   - The result goes to `.muxa/CHECK.md` for the agent. muxa does not type into the talk: the user is in that conversation.
5. **A passed proposal is one Decide item.** Its note lists the ops one per line; `settings_proposal` carries the ops and the settings' revision. `inbox-apply` sends them through `settings-update` with that revision and marks the item Done. When anything changed since, it is `conflict`: the user asks the Bot again.

## Consequences

- Every settings change still passes the user's tap and the revision check. A talk cannot touch connections, MCP servers, workers, env or another Bot.
- The Bot learns from talks too: its memory is written after a talk as after a run.
- A proposal can be stale by the time the user looks at it; Apply refuses it and nothing half-applies.
- Not built: a trial run of a proposed Routine before Apply. A proposed Routine is added turned off; the user turns it on, or runs it with Run now once it is on.
- Alternatives not taken:
  - The Bot edits `routines.toml` itself: it skips the user's decision and the loading rules.
  - A separate chat screen for Bots: the Task's Chat and Terminal already exist on every client.
  - Typing the check result into the talk: it would put muxa's words into the user's conversation.
