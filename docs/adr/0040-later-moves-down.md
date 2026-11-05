# 0040: Later moves an item down; the item screen separates triage from answers

## Status

Proposed. It changes ADR 0033 §4 (Later hides an item until a time) and §4a.2 (the titles, More, Redo and Change course). ADR 0041 adds Ask Agent to it.

## Context

- The developer works through the Inbox from the top down. Later is for an item that can wait: it should go below the rest, not disappear. A hidden item that comes back at a time is a reminder, which the Inbox does not need.
- A `review` offered Done (the Task is done and its session closes) and Keep open (the item closes, the Task stays). A `briefing` offered Done (the item closes). One word meant two results, and Keep open read like Later.
- The item screen put every answer in one row under the reply box, with More between them: Done, Send, More (Redo, Change course, Keep open, Later), Chat, Terminal. What the developer does with the item and what the agent asked were mixed.
- Redo and Change course only put a fixed sentence before the text that Send sends as it is. The developer can write that sentence.

## Decision

1. **Later moves the item to the bottom of the open items.** The item stays `open` and gets `laterAt` (epoch seconds). The host lists open items not moved down first (`ask` first, then newest), then the moved-down ones in the order they were moved, then the closed ones. An item stays down until it is answered. A new need is a new item, so it is listed in its place.
2. **`later` takes no time.** The host ignores `until`, so an older app's Later moves the item down too. A stored `later` item or entry from before this change is listed as open and moved down.
3. **Triage is in the top bar, the same on every item**, as Mail and Superhuman keep Archive and Snooze:
   - **✓** closes only the item: `keep-open` on a `review`, `dismiss` on a `failure`, an unreadable `ask` and a `briefing`. Green. Its accessible name is "Done".
   - **Later**, with the "Play Last" symbol of Music (`text.line.last.and.arrowtriangle.forward`): it moves the item to the end of the queue. Gray, the color of lower priority, not yellow: yellow is a snooze with a time.
   - **…** holds **Complete Task** (`accept`), **Drop Task** (`drop`) and **Open Session**. An answer that changes the Task names it, and sits one tap away.
4. **The bottom answers the agent.** Allow and Deny, Approve and Reject, Retry, or a question's options. Under them, one reply box with a send arrow: **Send** on an item with a session (the text goes to its agent as it is; on an `ask` the menu is denied first), **Ask Agent** on one without (ADR 0041). The placeholder says where the text goes.
5. **The source line opens the session**: its Chat where the agent has one, else its pane. The pane switches between Chat, Agent and Shell, so there are no separate Chat and Terminal buttons.
6. **A swipe right is ✓, a swipe left is Later**, as Mailbox, Spark and Superhuman do. A full swipe acts at once.
7. **Redo and Change course are gone** from the host, the CLI, muxa for tmux and both apps. The host no longer lists or takes them. An older app sends only what the host lists, so nothing breaks, and the protocol version stays.
8. The other wire action names stay.

## Consequences

- Nothing in the Inbox is hidden. An item moved down several times is still in the list.
- An older app still shows its time picker. Its Later moves the item down, and an older app hides nothing, because the host no longer sends `state: later`.
- Complete Task takes two taps. It closes the Task's session, so this is the answer least worth a slip.
- Alternatives not taken:
  - Keeping a time on Later: two meanings for one button again.
  - Moving an item down by one place (Move down, ADR 0027): the developer wants low-priority work out of the way, not reordered step by step.
  - A bottom toolbar for ✓ and Later (Mail's): it puts triage next to the reply box again.
  - A mode menu on the reply box (Send, Redo, Change course, Ask Agent): four ways to send one text, three of them to the same agent.
