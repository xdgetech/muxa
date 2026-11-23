# 0086: Bots as a main destination beside Home and Focus

## Status

Proposed (2026-11-23). Amends ADR 0012 and 0070 (navigation), and ADR 0082 (where Bots show). Builds on ADR 0083 to 0085. Wire: `protocol/PROTOCOL-v2.md`, "A Bot's work", the `muxa.bot` and `muxa.lead` labels, `SettingsBot.memory_history`.

## Context

ADR 0082 to 0085 put Bots in Settings: the user sets them up there and finds their items in the Inbox. The developer wants a Bot to be a place of its own, as Home and Focus are, with at least the UX of Orca bots. In Orca each Bot has a Home (Needs you, In progress, Planned, Closed), its chats, its automations and its files, and a new Bot starts with a chat. A mock and a review against the code showed which of that the wire already carries.

## Decision

1. **Bots is a main place.**
   - Mac: a third mode, Home ⇧⌘H · Focus ⇧⌘F · Bots ⇧⌘B. It replaces the Routines page; ⇧⌘O opens Bots.
   - iPhone and Android phones: a Bots tab replaces the Routines tab.
   - iPad and Android tablets: Bots replaces Routines in the sidebar.
   - Routines without a Bot are a section of Bots. Failed or held ones come first.
2. **A Bot's page has Home, Chats, Routines and Files.**
   - Home: Needs you, In progress, Planned, Recent. Clients build it from items, terminals, Routines and settings, by the rules in "A Bot's work".
   - Chats: its live talks. A chat is a talk Task (ADR 0083), but it opens on the Bot's page, with Chat, Terminal and Reader, not among Focus's Tasks.
   - Routines: its Routines with their last run.
   - Files: `soul.md`, and `memory.md` with the earlier versions and their times.
3. **New Bot, then a chat.** New Bot asks for a name, one sentence and the Workspace. Create sends `bot-add` and starts the Bot's first talk on its page. Routines come from that chat as proposals the user applies.
4. **Everything about a Bot stays on its page.** Its talks, and its Routine runs and members when they wait on the user, open on the Bot's page; talks leave Focus's tab strip and sidebar. Go to Task and the needs-you list still find them and open them there.
5. **A proposal shows beside its talk.** Its item has the talk's `terminal`. The client shows it above the chat with Apply, and it stays in the Inbox. Applying either one closes both.
6. **The host labels what belongs to a Bot.** A Bot's Routine runs carry `muxa.bot`. A team member carries `muxa.lead`. Clients never infer a Bot from paths or from the Routines list.
7. **Every waiting agent needs you.** The clients' "needs you" lists count Routine runs and members too, not only Tasks. A Task's menu is answered on the approval card. A Routine run or a member opens on its Bot's page, and the user answers in its terminal: the card follows a Task's Chat, which `chat-open` serves for Task agents only. That input is the user's own, to a muxa terminal (invariants 2 and 7). ADR 0085's "nothing is typed" still holds for muxa itself.

## Consequences

- Every item still reaches the one Inbox. A Bot's Home is a view of it, so the user can work from either place.
- Grouping is done by each client. The rules live in the protocol, and the two demo servers answer them, so Swift and Kotlin stay alike.
- Not built yet. Each needs a wire change and its own decision:
  - a list of past runs (a Routine has one last run);
  - a team step view (step and handoff counts live in host files);
  - closed chats (a closed Task leaves no record);
  - templates (they would copy Routine instructions into every client; a template should only pre-fill the first chat).
- Alternatives not taken:
  - Talks as Focus tabs: a Bot's work would be split over two places, and Focus is for the user's own Tasks.
  - A host `bot-view` request that does the grouping: one owner, but a new request and watch for data the clients already have. Reconsider it when the rules grow.
  - Keeping the Routines tab beside Bots: two places for most Routines.
  - Orca's auto-approve modes, "Who can ask", and event triggers: they break invariant 7, muxa has one user, and push relays are still undecided (ADR 0066).
