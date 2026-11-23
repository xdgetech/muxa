# 0082: A Bot owns Routines in one Workspace, with a Soul and a Memory

## Status

Proposed (2026-11-23). Builds on ADR 0026 (Routines), 0066 (Inbox), 0074 (Settings in the apps) and 0075 (connections per Workspace). Wire: `protocol/PROTOCOL-v2.md`, capability `bots`.

## Context

The developer sets up each Routine by hand: a `[[routine]]` entry and an instructions file. They want helpers that feel like Orca's bots or Grok Bot: a named helper per Workspace that watches its sources and puts things in the Inbox by itself, set up by talking to it, and later teams with a lead.

muxa removed a similar layer in C3 (the secretary, staff with a lead, delegates, workspace memory; ADR 0049–0061). One lesson carries over: output with no limit piles up in the Inbox.

Routines already run an agent on a schedule, check its output and publish it with dedup (ADR 0026). What they lack:

- An identity that lasts across runs. Each Routine has its own instructions and no memory but a cursor file.
- A way to run more than once a day. `routines/schedule.rs` keeps one occurrence per local date, so a Routine cannot watch Slack or GitHub during the day.
- Full items. `v2/feed.rs` copies only a title and a body. It drops links and the proposal.

## Decision

1. **A Bot is a named owner of Routines in one Workspace.** `[[bot]]` sits in the Workspace's `.muxa/routines.toml`, beside `[[routine]]`.
   - Fields: `name`, `description`, `worker`, `daily_items` (default 3), `enabled`.
   - A Routine names its Bot with `bot = "<name>"`. A Routine without `bot` runs as before, so nothing migrates.
   - A Bot gets the Workspace's connections and MCP servers, as every agent of the Workspace does (ADR 0075). It never gets another Workspace's.
2. **A Bot is not resident. Its Soul and Memory files carry it from one run to the next.** They live in `<ws>/.muxa/bots/<name>/`.
   - `soul.md` (at most 12 KiB): who the Bot is, its purpose, its scope and how it writes. It belongs to the user. It changes only by a settings op or by the user editing the file. A run never writes it.
   - `memory.md` (at most 16 KiB): what the Bot learned. It belongs to the Bot. A run returns a full replacement as `memory` in `output.json`. The host checks it and writes it atomically, and keeps the last 5 versions beside it.
   - Every run of the Bot's Routines gets both in `.muxa/TASK.md`, so each run starts as the same Bot.
3. **A Bot's runs put full items in the Inbox, within a daily limit.**
   - The output may make Read, Decide, Reply and To do items, with `due` and up to 5 links.
   - A Decide item keeps its proposal (action, target, content). Its actions are Start Task, with the goal taken from the proposal, and Delete. muxa itself never writes outside the host.
   - The host stamps the Workspace and the Bot. It never takes them from the output.
   - At most `daily_items` items per Bot and local day. The items past the limit become one Read item, "N more from <bot>".
   - The Inbox counts, per Bot, how many of its items were done and how many deleted. The apps show the counts beside the Bot.
4. **A Routine can run several times a day.** The minute and hour of `schedule` take lists, ranges and steps, as cron does: `*/30 9-18 * * 1-5` runs every 30 minutes on weekdays, in the Routine's `timezone`.
   - Times must be at least 15 minutes apart, across midnight too.
   - An occurrence is a local date and time, not a date. Missed occurrences are not caught up: a tick runs the latest one due. A state file written before this keeps a bare date, which counts as that whole day handled.
   - A run that finds nothing new publishes no item.
   - Cron, not new fields such as `every`: the settings ops, the wire and the apps already carry `schedule`, and agents know the form.
5. **`read_only = true` denies file edits to a Routine's agent.** The host writes Claude Code settings that deny `Edit` everywhere, as it does for a Task's read-only places (ADR 0068). It stops Claude Code's edit tools only. A shell command or a connection can still write, and `SECURITY.md` says so.
6. **Bots are edited like every other setting** (ADR 0074): `bot-add`, `bot-update` and `bot-remove`, and new fields on `routine-add` and `routine-update`, all or none against the revision. `bot-add` and `bot-update` take `soul_text`. `bot-memory-restore` puts back one of the kept versions. Every app edits them.

### Later decisions, each its own ADR

- Talking to a Bot to set it up: a terminal that starts from the Bot's Soul and Memory and ends with a list of settings ops that the user applies as a diff. The ops are limited to the Bot's own Workspace, and Routines arrive disabled.
- A Capture assistant: after the Capture parser adds an item, a short run suggests a kind, a Workspace, a due date, a split or a Bot to ask. The user accepts a suggestion with one tap. It gets no Workspace's connections.
- Teams: a lead Bot hands work to member Bots through the host, one level deep, with step and daily limits (from ADR 0051). Only the lead publishes items. This waits until Bots prove useful: at least 3 enabled Bots, and most of their items end as Done over two weeks.

## Consequences

- A Bot that watches Slack or GitHub reads through its own MCP servers and connections and keeps its place in `.muxa/routine/state.json`. muxa has no code for any source (ADR 0026). It does not reply in the source thread: the item links to it.
- Reaction time is the Routine's `every`, not seconds. Event triggers wait for a relay (ADR 0066, open decision 1).
- Role, scope and "ask before sending" are prompt text, not a boundary. The enforced limits are the Workspace's connections, the output's kinds, sizes and daily limit, and settings changes only by the user.
- Codex Bots are refused, as Codex Routines are.
- Alternatives not taken:
  - A resident session per Bot: it uses tokens all the time and its context degrades. Soul and Memory files give the same continuity.
  - A muxa MCP server that a run calls to add items: any same-user process reaches `server.sock`, so a token is no boundary, and it skips the output check and retry. File output is enough until a team needs calls during a run.
  - Bots that message each other, or type into each other's terminals: invariants 2 and 7.
  - Memory changes as items to review: ADR 0053 did this and it only added items. Kept versions let the user undo instead.
