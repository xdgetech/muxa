# 0066: Home is an Inbox of things to do, fed by Capture, Tasks and Routines

## Status

Proposed (2026-11-21). Product description: `docs/next.md`.

ADR 0096 changes §7's Later: it moves an item to the bottom, and nothing is hidden.

The removal is done (2026-11-22, plan C3). Two decisions came with it:

- Workspace memory (ADR 0053) is removed too. Its copy into a run's folder and its `memory` items ran through the assistant's notes code. ADR 0053 is superseded.
- `ask-agent` and level 6 are gone with ADR 0041. The root Ask session of ADR 0054 stays, without the secretary: `start-session {root: true}` opens a plain one.

Supersedes, once the removal below is done: ADR 0040, 0041, 0045, 0046 (for removed kinds), 0048 (requests and assistant runs; Captures and Work items become the Inbox), 0049, 0050, 0051, 0052, 0054, 0056, 0057, 0058, 0059, 0060, 0061, and ADR 0033's item kinds and order. ADR 0035's sidebar modes become Home and Focus. ADR 0028 keeps only what Codex Routines use; its delegate and staff parts go.

## Context

The developer used three things daily: the terminal on the phone, the Inbox, and Routines. The secretary was used for one job: to pile up work. Staff, delegates, the overall assistant's plans and the Today queue were built but not used.

The Inbox also mixed two different things. A session's question (ADR 0033 `ask`) lasts minutes and is answered in the session. A reply to write or a result to review can wait for days. Both made items, so the list filled with notices.

## Decision

1. **Home is the main mode, Focus is the mode for one Task.** Home holds the Inbox and the Routines. Focus holds a Task's panes, terminal and Reader.
2. **An Inbox item is something the user has to do.** Kinds: Review, Reply, Decide, Read, To do, Failed.
3. **A waiting session is not an item.** It shows as Needs you: a strip in Home, the top of the sidebar in Focus, `F8` to go to the next one. So does a turn that ended with nothing to review. Needs you is pushed to the phone also while the app is closed (decided 2026-11-21), so ADR 0008's deferred push becomes required work.
4. **A Task makes items at two moments.**
   - **Review**, one per Task at a time, when the Task enters review: its check passed (ADR 0019), its agent opened a draft PR, or the user moved it. Approve closes the Task; on a dirty worktree it first asks ADR 0043's question. Request changes sends the comment to the agent, resolves the item and puts the Task back to open; the next review makes a new item. A Task closed elsewhere (its PR merged, `muxa-cli task done`) resolves its item.
   - **Failed**, when the agent fails to start, crashes, or its loop stops on an error (ADR 0033 `failure`).
5. **Routines are silent while they work.** A Routine run is not a Task and is not listed under its Workspace; it shows only in Routines (decided 2026-11-21). Each Routine sets two things: the kind of its output (Read, Decide, or none) and when it makes an item (always, or only when its output says so). A failure after the retries always makes one Failed item.
6. **Capture replaces the secretary.** One line from any device adds an item: a hotkey window on the Mac, the share sheet on phones, `muxa capture` in a terminal. Parsing rules:
   - a first word `reply`, `read` or `decide` sets the kind, else To do;
   - `#name` sets the Workspace only when a Workspace has that name, else it stays text;
   - `due <day or date>` sets the due date in the device's time zone;
   - the parsed fields show as chips before the item is added, and can be changed.

   Capture may suggest a Task of that Workspace by its name. Nothing reorders items or decides for the user.
7. **The user owns the order.** The list is in manual order: drag, or `⌥↑` / `⌥↓`. New items go on top. A due date is optional. Later hides an item until a chosen time; it then returns on top.

### Old item kinds

| ADR 0033 / 0048 kind | Becomes |
|---|---|
| `ask` | Needs you |
| `review` | Review |
| `failure` | Failed |
| `briefing` | Read |
| `proposal` from a Routine | Decide |
| `memory` (ADR 0053, `review` mode) | Decide |
| `result`, `write`, `handoff`, `sort`, `delegate`, request `proposal` | removed with the assistant layer |

### Removal

Each step has its own commit and checks, on the host, the CLI, iOS and Android together.

1. Stop the secretary's resident session and the plan schedule (`resident = false`, `plans = []`).
2. Hide Today, plans, policies, hypotheses and staff in every client.
3. **Nothing is migrated** (decided 2026-11-21). At the switch, existing Captures, Work items and open Inbox items are dropped, and the new Inbox starts empty.
4. Raise the `work:<level>` capability. Remove the old wire values, host code and stores.

## Consequences

- The Inbox gets shorter and each item asks for one action.
- Needs you carries what the Inbox used to: an agent's question. ADR 0008's banner covers it while an app is open. Push while the app is closed needs a relay for APNs and FCM; its design is a separate ADR.
- Manual order replaces ADR 0033's order and ADR 0040's Later. It does not map to Linear priority; a later Linear sync sends states only.
- Removing the layer deletes a large amount of host, wire and app code. ADR 0067's server is easier to build without it.

## Alternatives not taken

- **A hosted assistant (Grok Bot) as secretary and business runner.** All Bots on an account share one cloud computer and every connection; xAI's docs say Bots are not a security boundary (docs.x.ai, "Approvals, security and privacy"). Business data would leave the host. The developer chose to keep company Routines on their own Mac.
- **Priority levels (Urgent to Low).** Tried in mockups. Drag order was preferred.

## Not decided

1. How push reaches the phone while the app is closed: the relay, what it may see, and who runs it.
