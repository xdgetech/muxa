# 0084: A Capture assistant, Ask a Bot, and Routines that run on review

## Status

Proposed (2026-11-23). Amends ADR 0066 decision 6 (Capture). Builds on ADR 0082 and 0083. Wire: `protocol/PROTOCOL-v2.md`, "Capture assistant", Item `suggestion`, `inbox-suggestion`, RoutineSpec `on`, op `assistant-update`.

## Context

Capture reads a line by fixed rules (ADR 0066): the first word sets the kind, `#name` the Workspace, `due` the date. It is fast and predictable, and it does not understand the line. The developer wants Capture to be smarter. muxa removed its overall assistant in C3, but not because it was unused (ADR 0082 notes this). ADR 0066 keeps one rule: nothing is decided for the user.

Two more requests fit here: asking a Bot about an item, and running a Bot when a Task is ready for review.

## Decision

1. **The parser stays.** An item goes into the Inbox at once, as today.
2. **The Capture assistant suggests; the user decides.** With `[assistant] enabled = true` in `workspaces.toml`, each new captured item gets one short Claude Code run (`v2/assistant.rs`).
   - It has no Workspace, no connections and no MCP servers (an empty `--strict-mcp-config`), and may not edit the folder that holds `workspaces.toml`.
   - It reads the item, the Workspace names, their Bots and descriptions, its own `soul.md` and `memory.md` (in `assistant/` beside `workspaces.toml`), and the user's last 30 decisions.
   - It writes a suggestion: kind, Workspace, due, note, a split into 2 to 5 items, a goal for Start Task, or a Bot to ask. The host drops a Workspace or Bot that does not exist.
   - The suggestion shows on the item. `inbox-suggestion` accepts or dismisses it; the decision is kept for the next run.
   - At most 3 runs at a time; a run ends after 10 minutes. Turning it on does not run it on older items.
3. **Ask a Bot is a talk.** On an item, the client starts a talk (ADR 0083) with the Bot and the item as the goal. No new request.
4. **A Routine may run on review.** `on = "review"` on a Routine with no schedule runs it each time a Task of its Workspace gets a Review item. The run's brief names the Task, and Claude Code may read the Task's places and edit none of them.
5. **Settings** edit the assistant like everything else (ADR 0074): `assistant-update` sets `enabled`, `worker` and the Soul.

## Consequences

- Capture stays instant; the suggestion comes some seconds later and changes nothing on its own.
- The assistant costs one agent run per capture while it is on. It is off by default.
- The assistant sees item text and Workspace and Bot names only. Work that needs a Workspace's access goes to that Workspace's Bot or a Task.
- A review Routine reads the Task's places through Claude Code's additional directories; a shell command can still write there, as `SECURITY.md` says for every read-only rule.
- Alternatives not taken:
  - Replacing the parser with the assistant: Capture would wait for a model, and a wrong guess would change the item.
  - Giving the assistant every Workspace's connections: one prompt would reach every account. ADR 0075 keeps access per Workspace.
  - Event triggers from outside (webhooks): they still wait for a relay (ADR 0066, open decision 1).
