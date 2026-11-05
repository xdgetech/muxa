# Inbox UI specification

ADR 0033: the Inbox is a list of questions. The host, the CLI, muxa for tmux and both apps show it when the host has `work:3`. In the apps it is one of the sidebar's three modes (ADR 0035).

## The Inbox (ADR 0033)

### The list

```
Inbox (3)
───────────────────────────
● Approve terraform apply
  lisk-infra · 4m
● Ready for review
  reduce GA cost · 1h
○ Morning briefing
  Lisk · 8:00
```

- One row per item: the question, then the Task title or Routine, and the age. `●` is unread.
- Order: `ask` items first, then newest first. Items moved down with Later come last, in the order they were moved (ADR 0040).
- A swipe is Later: the item moves to the bottom at once. It stays there until it is answered.
- Empty: "Nothing needs you", and a link to All Tasks.

### The item screen

```
┌ Ready for review ──────────┐
│ reduce GA cost · lisk-app  │
│                            │
│ Moved the GA4 export to    │
│ daily batches. Tests pass. │
│ PR #817.                   │
│                            │
│ Attachments                │
│ ± Changes  4 files +120 −40│
│ ▤ plan.md  docs            │
│ ▧ chart.png  docs          │
│ ↗ PR #817  github.com      │
│                            │
│ ┌ reply… ────────────────┐ │
│ └────────────────────────┘ │
│ [Complete Task] [Send] ⋯ ✓ ⤓│
└────────────────────────────┘
```

1. The question and the source.
2. The summary: the agent's last message for the turn, as Markdown.
3. Attachments as a list with file names. Each opens in the Reader and returns to the item; a link opens in the browser.
4. The reply box. Send, Redo and Change course send its text to the item's agent; Ask Agent sends it with the item to the workspace's Ask session (ADR 0041). They need a text.
5. The answers for the kind (ADR 0033 §4a):

| Kind | Buttons |
|---|---|
| `ask` | Allow, Deny, Deny & send; a question shows its options |
| `review` | Complete Task, Send; Redo, Change course and Ask Agent under More; ✓ |
| `failure` | Retry, Send; Change course, Drop Task and Ask Agent under More; ✓ |
| `proposal` | Approve, Reject, Ask Agent |
| `briefing` | Ask Agent, ✓ |

Every kind also has Later (⤓). An `ask` shows Ask Agent as a button, beside Allow, Deny and Deny & send. Ask Agent leaves the item open and opens the Ask session's Chat: the workspace's one ask session at its root, `ask-<workspace>`, which the host starts on the first request (ADR 0041). ✓ and Later are icon buttons and never under More (ADR 0040). ✓ closes only the item: the Task stays as it is. Complete Task sets the Task done and closes its session; Drop Task drops it. More exists only when two or more answers would be hidden. Complete Task and Drop Task ask for no confirmation, because they only set the Task's status; Approve on a proposal still shows the action, target and content first. An answer returns to the list.

### Phone and tablet (ADR 0035)

- The sidebar shows one mode: **Inbox**, **Tasks** or **Sessions**. Inbox mode is the host's full list of items, in the host's order, with no fold. The Inbox count is on the mode control.
- Tablet: the list is the sidebar and the selected item is the detail. At connect, and on a switch to Inbox mode, the first item is selected (which marks it read). After an answer the next item opens.
- Phone: the list is the first screen. The item is pushed; an answer returns to the list. Back goes pane → item → list.
- The item's **Chat** and **Terminal** show its session: Chat opens the Chat view, Terminal the agent's pane. On a phone they push over the item; on a tablet the pane fills the detail and the sidebar collapses. An item never opens the Shell first.
- The summary is Markdown (ADR 0033 §4a.5). Its `http` and `https` links open in the browser. A link or a file name that matches an attachment opens that attachment in the Reader.
- The attachments are a list under the summary: Changes with the file and line counts, then each doc and image by file name and folder, then the links by title. Evidence opens in the Reader and returns to the item.
- Labels name the result (ADR 0033 §4a.2, ADR 0040). `accept` is **Complete Task** and `drop` is **Drop Task**. `keep-open` and `dismiss` are **✓**, named "Done" for VoiceOver and TalkBack. `later` is **⤓**, named "Later". `reply` is **Send**, or **Deny & Send** on an `ask`. On an `ask`, `approve` is **Allow** and `reject` is **Deny**. A proposal keeps **Approve** and **Reject**.
- At connect, Inbox and Tasks mode stay on their list, also when it is empty ("Nothing needs you"). Sessions mode opens the default pane.
- A host below `work:3` gets "This host needs a newer muxa-bridge for the Inbox", with Tasks and Sessions.
- The reply box shows only when the item takes a text answer. Send, Redo and Change Course need its text.
- The main answer is `reply` while the reply box has text (ADR 0033 §4a.3). Otherwise it is the host's first answer, unless that is `choose` or `later`. muxa for tmux uses the same rule. The main answer is the prominent button.
- iPad keys: Command+T Inbox mode, Command+L Tasks mode, Command+Shift+S Sessions mode, Command+0 shows or hides the sidebar, Command+, Settings. In the Inbox: Command+Up/Down move between items, Return opens an item on a phone, Command+Return sends the main answer, Command+O opens the item's terminal, Command+Shift+L moves the item down.
- Android: Back from an attachment closes the Reader and returns to the item.
- Android keys: the same with Ctrl+Shift (Ctrl+Shift+T, L, S, 0, O, Up/Down), Enter opens, Ctrl+Shift+Enter sends the main answer, Ctrl+Shift+Alt+L moves the item down. A swipe in either direction moves it down.
- Demo: Try the Demo, the showcase and the test demo play a `work:3` host. UI tests pick the mode with `-mode inbox|tasks|sessions` on iOS and `--es mode inbox|tasks|sessions` on Android.

### tmux

- The sidebar shows the Inbox section above the session tree: the count and the first open items. `Enter` on one opens it in `prefix + T`.
- `prefix + T` shows the list with the selected item below it: its question, summary, numbered evidence and keys.
- Keys: `a` the item's main answer (Allow, Complete Task, Retry or Approve), `s` Send (Deny & send on a permission), `r` Deny, Reject or Redo, `c` Change course, `p` and `n` ✓ Done (only the item), `x` Drop Task, `g` Ask agent (the item and a text to the workspace's Ask session; the item stays open), `l` Later (moves the item down, with no time), `1`–`9` open evidence (the diff first) or choose a question's option, `Enter` or `o` open the session. `space` and `b` scroll the item by a page, `J` and `K` by a line, and `v` shows it whole in the pager. Send, Redo, Change course and Ask agent ask for the text. Only the keys the item takes are listed, in the host's order of answers.
- Labels name the result (ADR 0033 §4a, ADR 0040): `accept` is "Complete Task", `drop` "Drop Task", `keep-open` and `dismiss` "✓ Done", `reply` is "Send" ("Deny & send" on an `ask`), and on an `ask` `approve` is "Allow" and `reject` "Deny". A proposal keeps Approve and Reject.

## Tasks and Sessions modes

- Tasks mode lists Routines first, then every Task grouped by what it needs: Needs attention, Ready for review, Running, No agent, Closed (`protocol/fixtures/inbox/groups.jsonl`). The detail is the Task's detail. A Routine's detail shows its workspace, trigger, enabled flag, worker, project, next run, last success, last failure, a running run with Open Terminal, and Run now. Run now on a bridge without the daemon shows the host's message.
- Sessions mode lists only sessions with no Task association, grouped by repo, with each repo's New Task and Ask (ADR 0025, 0031). The session switcher searches every session. A session's detail is its pane with the Chat / Agent / Shell switch; a Sessions row opens the session's last view.
- Settings holds the hosts (connect, add, edit, delete, reorder, Disconnect), Try the Demo, Leave Demo and the version. A status row under the mode control shows the connection and opens Settings. In the demo it reads "Demo" with Leave Demo.
- ADR 0033 slice I4 removed the Task's Inbox queue (ADR 0027 §4): Move down, the record's `inbox` field, `task-inbox` and the local Snooze. Later exists only on an Inbox item (`update-inbox` `later`). Since ADR 0040 it moves the item down instead of hiding it until a time.


Association is the classification boundary. Manually launched AI remains manual; typing by hand inside a Task does not change its classification. Manual approval waits can show a Sessions badge without generating Inbox items. Managed work needing human attention returns to Inbox as a reference to the same Task. Routine output enters Inbox as briefings/proposals, while the collector's execution remains under Routine history. No automatic Task creation or session reclassification is implied.

The domain model is defined in [Inbox, Tasks and Sessions](work-model.md). Session is an execution location that can host manual or managed work; only the default manual-session screen is filtered. Task/run identity survives human versus AI input changes. Recreating a session for an existing no-session Task and adopting manual work into a Task are separate future host operations.
