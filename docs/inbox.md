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
- A swipe right is ✓ (the item closes), a swipe left is Later: the item moves to the bottom at once and stays there until it is answered (ADR 0040 §6). A full swipe acts at once.
- Empty: "Nothing needs you", and a link to All Tasks.

### The item screen

```
┌← Inbox     Review    ⤓  ✓  … ┐
│ Ready for review: Reduce GA   │
│ Reduce GA cost · lisk-app ›   │ ← opens the session
│                               │
│ Moved the GA4 export to ...   │
│ Attachments                   │
│ ± Changes  4 files +120 −40   │
│ ▤ plan.md  docs               │
│                               │
│ ┌ Reply to the agent…    (↑)┐ │
│ └───────────────────────────┘ │
└───────────────────────────────┘
```

1. The question, and the source. With a session, the source opens it: its Chat where the agent has one, else its pane, which switches between Chat, Agent and Shell.
2. The summary: the agent's last message for the turn, as Markdown.
3. Attachments as a list with file names. Each opens in the Reader and returns to the item; a link opens in the browser.
4. The top bar is triage, the same on every item (ADR 0040 §3): **Later** (gray, the "Play Last" symbol) moves the item to the bottom; **✓** (green) closes only the item; **…** holds **Complete Task**, **Drop Task** and **Open Session**.
5. The bottom answers the agent: its decisions, then one reply box with a send arrow.

| Kind | Bottom | … |
|---|---|---|
| `ask` | Allow, Deny; a question shows its options; reply box: Deny & Send | Open Session |
| `review` | reply box: Send | Complete Task, Open Session |
| `failure` | Retry; reply box: Send | Drop Task, Open Session |
| `proposal` | Approve, Reject; reply box: Ask Agent | — |
| `briefing` | reply box: Ask Agent | — |

The reply box sends **Send** on an item with a session: the text goes to its agent as it is. On one without a session it sends **Ask Agent**: the item and the text go to the workspace's Ask session, `ask-<workspace>`, which the host starts on the first request; the item stays open and the session's Chat opens (ADR 0041). The placeholder says where the text goes. Complete Task and Drop Task ask for no confirmation, because they only set the Task's status; Approve on a proposal still shows the action, target and content first. An answer returns to the list.

### Phone and tablet (ADR 0035)

- The sidebar shows one mode: **Inbox**, **Tasks** or **Sessions**. Inbox mode is the host's full list of items, in the host's order, with no fold. The Inbox count is on the mode control.
- Tablet: the list is the sidebar and the selected item is the detail. At connect, and on a switch to Inbox mode, the first item is selected (which marks it read). After an answer the next item opens.
- Phone: the list is the first screen. The item is pushed; an answer returns to the list. Back goes pane → item → list.
- The item's session opens from its source line or Open Session. On a phone it pushes over the item; on a tablet the pane fills the detail and the sidebar collapses. An item never opens the Shell first.
- The summary is Markdown (ADR 0033 §4a.5). Its `http` and `https` links open in the browser. A link or a file name that matches an attachment opens that attachment in the Reader.
- The attachments are a list under the summary: Changes with the file and line counts, then each doc and image by file name and folder, then the links by title. Evidence opens in the Reader and returns to the item.
- Labels name the result (ADR 0033 §4a.2, ADR 0040). `accept` is **Complete Task** and `drop` is **Drop Task**. `keep-open` and `dismiss` are **✓**, named "Done" for VoiceOver and TalkBack. `later` is the "Play Last" symbol, named "Later". `reply` is **Send**, or **Deny & Send** on an `ask`. On an `ask`, `approve` is **Allow** and `reject` is **Deny**. A proposal keeps **Approve** and **Reject**.
- At connect, Inbox and Tasks mode stay on their list, also when it is empty ("Nothing needs you"). Sessions mode opens the default pane.
- A host below `work:3` gets "This host needs a newer muxa-bridge for the Inbox", with Tasks and Sessions.
- The reply box shows only when the item takes a text answer: `reply` or `ask-agent`. The send arrow needs its text.
- The main answer is the reply box's action while it has text (ADR 0033 §4a.3). Otherwise it is the host's first answer, unless that is `choose`, `later` or a text answer. A decision that is the main answer is the prominent button.
- iPad keys: Command+T Inbox mode, Command+L Tasks mode, Command+Shift+S Sessions mode, Command+0 shows or hides the sidebar, Command+, Settings. In the Inbox: Command+Up/Down move between items, Return opens an item on a phone, Command+Return sends the main answer, Command+O opens the item's terminal, Command+Shift+L moves the item down.
- Android: Back from an attachment closes the Reader and returns to the item.
- Android keys: the same with Ctrl+Shift (Ctrl+Shift+T, L, S, 0, O, Up/Down), Enter opens, Ctrl+Shift+Enter sends the main answer, Ctrl+Shift+Alt+L moves the item down. A swipe right is ✓, a swipe left moves it down.
- Demo: Try the Demo, the showcase and the test demo play a `work:3` host. UI tests pick the mode with `-mode inbox|tasks|sessions` on iOS and `--es mode inbox|tasks|sessions` on Android.

### tmux

- The sidebar shows the Inbox section above the session tree: the count and the first open items. `Enter` on one opens it in `prefix + T`.
- `prefix + T` shows the list with the selected item below it: its question, summary, numbered evidence and keys.
- The keys follow the apps' item screen (ADR 0040), on two lines. Triage: `l` Later (moves the item down, with no time), `d` ✓ Done (only the item), `c` Complete Task, `x` Drop Task, `o` Open Session. Answers: `a` the first decision (Allow, Approve or Retry; never Complete Task), `r` Deny or Reject, `1`–`9` a question's option, `s` the reply box (Send with a session, Deny & Send on a permission, Ask Agent without a session; it asks for the text). Otherwise `1`–`9` open evidence (the diff first). `Enter` also opens the session. `space` and `b` scroll the item by a page, `J` and `K` by a line, and `v` shows it whole in the pager. Only the keys the item takes are listed.
- Labels are the apps' labels (ADR 0033 §4a, ADR 0040): `accept` is "Complete Task", `drop` "Drop Task", `keep-open` and `dismiss` "✓ Done", `later` "Later", `ask-agent` "Ask Agent", `reply` is "Send" ("Deny & Send" on an `ask`), and on an `ask` `approve` is "Allow" and `reject` "Deny". A proposal keeps Approve and Reject.

## Tasks and Sessions modes

- Tasks mode lists Routines first, then every Task grouped by what it needs: Needs attention, Ready for review, Running, No agent, Closed (`protocol/fixtures/inbox/groups.jsonl`). Closed lists a Task for a day after its last change; an item can still open an older one. A Task with a session opens its pane, as a Sessions row does; its detail is Task Info in the pane's More menu (ADR 0044). A Task without a session, or a closed one, shows its detail. A Routine's detail shows its workspace, trigger, enabled flag, worker, project, next run, last success, last failure, a running run with Open Terminal, and Run now. Run now on a bridge without the daemon shows the host's message.
- Sessions mode lists only sessions with no Task association, grouped by repo, with each repo's New Task and Ask (ADR 0025, 0031). The session switcher searches every session. A session's detail is its pane with the Chat / Agent / Shell switch; a Sessions row opens the session's last view.
- Settings holds the hosts (connect, add, edit, delete, reorder, Disconnect), Try the Demo, Leave Demo and the version. A status row under the mode control shows the connection and opens Settings. In the demo it reads "Demo" with Leave Demo.
- ADR 0033 slice I4 removed the Task's Inbox queue (ADR 0027 §4): Move down, the record's `inbox` field, `task-inbox` and the local Snooze. Later exists only on an Inbox item (`update-inbox` `later`). Since ADR 0040 it moves the item down instead of hiding it until a time.


Association is the classification boundary. Manually launched AI remains manual; typing by hand inside a Task does not change its classification. Manual approval waits can show a Sessions badge without generating Inbox items. Managed work needing human attention returns to Inbox as a reference to the same Task. Routine output enters Inbox as briefings/proposals, while the collector's execution remains under Routine history. No automatic Task creation or session reclassification is implied.

The domain model is defined in [Inbox, Tasks and Sessions](work-model.md). Session is an execution location that can host manual or managed work; only the default manual-session screen is filtered. Task/run identity survives human versus AI input changes. Recreating a session for an existing no-session Task and adopting manual work into a Task are separate future host operations.
