# 0033: The Inbox is a list of questions

## Status

Proposed. The developer chose this shape on 2026-11-04. It replaces the Inbox list of ADR 0025 ("List first, with explicit disposition"), ADR 0027 §4 (Move down and Later on the Task record), and what is left of ADR 0023 in the Inbox (need labels, groups, the local Snooze). The manual Sessions list (ADR 0025, ADR 0027 §4) and Routines (ADR 0026) stay.

ADR 0040 changes §4 (Later moves an item down instead of hiding it) and §4a.2 (✓, Complete Task, Drop Task, and More).

## Context

- The Inbox shows the state of each Task: groups, needs, a queue order, Move down. It does not say what the user must decide, and it is hard to read on a phone.
- A Task stays in the list while it is open, also after the user has answered. A closed session leaves an `open` Task with "no session", which the list keeps showing.
- The developer wants something like a to-do list or email: each item asks for one decision, shows its evidence (the diff, docs, images), and offers the answers. The UI must be simple, most of all on a phone.

## Decision

### 1. An item is one question, stored on the host

An Inbox item exists only while someone waits on the user. It is a stored entry, the same record as a Routine's briefings and proposals (PROTOCOL.md, stored Inbox entries), with new kinds. Every device shows the same items and the same read and answered state.

| Kind | Made when | Answers |
|---|---|---|
| `ask` | a Task's agent waits on a permission, a question or a plan (ADR 0009) | Approve, Reject, or a reply |
| `review` | a Task's agent finished a turn, or its loop passed (ADR 0019) | Accept, Redo, Change course |
| `failure` | a Task's agent stopped with an error, its loop stopped, or its start failed (the session did not start, or the start prompt was never typed) | Retry, Change course, Drop; without a session only Drop |
| `proposal` | a Routine proposes an action (ADR 0026) | Approve, Reject |
| `briefing` | a Routine publishes a briefing | Done |

- Accept marks the Task done. An item with an open PR also offers Keep open, which answers the item and leaves the Task open.
- Redo and Change course send the user's text to the agent as the next prompt, through the adapter that sends Chat messages (ADR 0005). Redo is feedback on the same goal; Change course replaces the direction. Both need a text.
- Retry restarts the agent with a handoff (ADR 0018). Drop marks the Task dropped.
- A failed start has no agent state to make an item from, so the failure makes the item itself. With no session, Retry and Change course could not reach an agent, so the item offers only Drop and Dismiss. The item resolves when the agent runs, as any Task item does.
- Approve and Reject on an `ask` use the approval contract of ADR 0009, unchanged. Where the adapter cannot answer, the item offers Open Terminal instead.
- Every item also offers Later (until a time) and Open Terminal when its Task has a pane.

Running work, Tasks with no agent and Tasks with no session make no item. They are in All Tasks.

### 2. Items come from adapter states, once each

- The host makes an item when the state an adapter supplies starts a new need: `waiting`, a finished turn, `error`, or a loop result. It never infers a need from terminal content (invariant 6).
- One Task has at most one unanswered item. A newer need replaces the older item.
- When the need ends without an answer from the Inbox, the item closes as `resolved`. Example: the user approved in the terminal, so the agent runs again.
- The daemon makes items while no client is connected. Without the daemon, a running bridge makes them, as it serves Task records (ADR 0019).

### 3. What an item shows

- A one-line question: the agent's request, "Ready for review: <title>", or what failed.
- A summary: the agent's last message for that turn, at most 4 KB. It is stored in the item, like `TASK.md` is stored on the host. It never reaches logs (invariant 5).
- Attachments the Reader opens (ADR 0006, 0007, 0010, 0011): the pane's changes (uncommitted, else the branch's own), the Markdown files and images among them, and the PR link. `TASK.md` stays in the pane's docs.
- Its source: the Task title, the repo and the age.

An item is unread until a device opens it. Opening an item never answers it.

### 4. The UI

- **List:** like a mail list. Each row is the question, then the source and the age. A dot marks unread. Newest first, except that `ask` items come first, because an agent is blocked on them. The only gesture is a swipe for Later.
- **Item screen:** the question, the summary, the attachments as chips, a reply box and two or three answer buttons. An attachment opens in the Reader and returns to the item. An answer removes the item from the list.
- **tmux:** the sidebar's Inbox section and `prefix + T` show the same list. `Enter` opens an item in a popup. Single keys answer: `a` the main answer, `r` Reject or Redo, `c` Change course, `x` Drop, `l` Later, `o` open the session. Diffs and docs open in a pager.

### 4a. Answers say what they do (added 2026-11-04)

The developer found the first buttons unclear: to reply and let the Task go on, it was not clear whether to press Approve or Reject, or Redo. Documents and images the agent made had no link on a phone or a tablet. So:

1. **Reply.** A new answer `reply {text}` sends the text to the agent as it is, with no "Redo" or "Change course" preface, and the Task goes on. A `review` and a `failure` offer it. An `ask` whose adapter answers permissions offers it too: it picks the prompt's cancel option, then sends the text.
2. **Labels name the result,** in every client:
   - `accept` is **Done**, with "closes the Task".
   - `reply` is **Send**. On an `ask` it is **Deny & send**.
   - On an `ask`, `approve` is **Allow** and `reject` is **Deny**. A `proposal` keeps Approve and Reject.
   - Redo, Change course and Keep open move under More.
3. **The main answer** is `reply` while the reply box has text. Otherwise it is the host's first answer, as before.
4. **Evidence outside git.** In a Task directory that is not a git repo, the item attaches the documents and images created or changed in the turn, by modification time, at most 20. In any Task, it also attaches files the summary names that exist inside the pane's directory, and the `http` and `https` links in the summary. `.muxa/` and `.claude/` are never attached.
5. **Links in the apps.** The item screen renders the summary as Markdown. Links open in the browser. A file name that matches an attachment opens in the Reader and returns to the item. Attachments show as a list with file names, not only chips.
6. **tmux:** `s` sends a reply (it asks for the text), and the key list uses the same labels.

Old clients drop the unknown `reply` answer, as PROTOCOL.md says, so no capability is needed.

### 5. What is removed

- Move down, and the Task record's `inbox {order, until, since}`. Later moves to the item.
- The queue rule (`protocol/fixtures/inbox/queue.jsonl`), need labels and groups in the Inbox. All Tasks keeps its groups.
- The local Snooze, and the Triage remains of ADR 0023.

The wire changes are listed in `protocol/PROTOCOL.md` when slice I0 lands. muxa is pre-alpha with one user, so the old fields are removed, not kept beside the new ones.

## Delivery slices

| Slice | Scope |
|---|---|
| I0 | host and CLI: the new kinds, items from adapter states, answers through `update-inbox`, `muxa-cli inbox` and `inbox answer` |
| I1 | muxa for tmux: the sidebar's Inbox section and the item popup |
| I2 | iOS: the list and the item screen |
| I3 | Android: the same |
| I4 | remove Move down, the Task's `inbox` field, the queue code and the local Snooze |
| I5 | host, CLI and muxa for tmux: `reply`, evidence outside git and from the summary, the labels in tmux (§4a) |
| I6 | iOS and Android: the labels, Send, the Markdown summary with links, the attachment list (§4a) |

## Consequences

- One record serves every question, from a Task or a Routine. The list needs no queue rule.
- An answered item stays answered, also while its Task stays open. A Task with no session no longer shows up as something to do.
- The host stores agent summaries, so the state directory holds more conversation text. It stays private to the host, like transcripts.
- Attachments are the pane's changes, docs and images through the existing endpoints, counted when the item is made. A diff of one turn alone is not kept; it would need per-turn snapshots.
- Rejected: deriving items from the Task's state on each list. It is less code, but an answered review comes back while the Task is open, which is the confusion this ADR removes.
- Rejected: keeping Move down. Later and the newest-first order cover it with one action fewer.
