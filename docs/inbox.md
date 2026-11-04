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
- Order: `ask` items first, then newest first.
- A swipe asks for Later: in 1 hour, tomorrow 9:00, or a time.
- Empty: "Nothing needs you", and a link to All Tasks.

### The item screen

```
┌ Ready for review ──────────┐
│ reduce GA cost · lisk-app  │
│                            │
│ "Moved the GA4 export to   │
│  daily batches. Tests      │
│  pass. PR #817."           │
│                            │
│ [Diff +120 −40] [PR #817]  │
│ [plan.md] [chart.png]      │
│                            │
│ ┌ reply… ────────────────┐ │
│ └────────────────────────┘ │
│ [ Accept ] [ Redo ]        │
└────────────────────────────┘
```

1. The question and the source.
2. The summary: the agent's last message for the turn.
3. Attachments as chips. Each opens in the Reader and returns to the item.
4. The reply box. Redo and Change course send its text to the agent, so they need one.
5. The answers for the kind:

| Kind | Buttons |
|---|---|
| `ask` | Approve, Reject; a reply answers a question |
| `review` | Accept, Redo; Change course and Keep open under More |
| `failure` | Retry, Change course; Drop under More |
| `proposal` | Approve, Reject |
| `briefing` | Done |

More also holds Later and Open Terminal. Accept and Drop ask for no confirmation, because they only set the Task's status; Approve on a proposal still shows the action, target and content first. An answer returns to the list.

### Phone and tablet (ADR 0035)

- The sidebar shows one mode: **Inbox**, **Tasks** or **Sessions**. Inbox mode is the host's full list of items, in the host's order, with no fold. The Inbox count is on the mode control.
- Tablet: the list is the sidebar and the selected item is the detail. At connect, and on a switch to Inbox mode, the first item is selected (which marks it read). After an answer the next item opens.
- Phone: the list is the first screen. The item is pushed; an answer returns to the list. Back goes pane → item → list.
- The item's **Chat** and **Terminal** show its session: Chat opens the Chat view, Terminal the agent's pane. On a phone they push over the item; on a tablet the pane fills the detail and the sidebar collapses. An item never opens the Shell first.
- Evidence opens in the Reader and returns to the item.
- At connect, Inbox and Tasks mode stay on their list, also when it is empty ("Nothing needs you"). Sessions mode opens the default pane.
- A host below `work:3` gets "This host needs a newer muxa-bridge for the Inbox", with Tasks and Sessions.
- The reply box shows only when the item takes a text answer. The summary is plain text until slice I6 (§4a of ADR 0033).
- The main answer is the host's first answer, unless that is `choose` or `later`. muxa for tmux uses the same rule.
- iPad keys: Command+T Inbox mode, Command+L Tasks mode, Command+Shift+S Sessions mode, Command+0 shows or hides the sidebar, Command+, Settings. In the Inbox: Command+Up/Down move between items, Return opens an item on a phone, Command+Return sends the main answer, Command+O opens the item's terminal, Command+Shift+L asks for Later.
- Android keys: the same with Ctrl+Shift (Ctrl+Shift+T, L, S, 0, O, Up/Down), Enter opens, Ctrl+Shift+Enter sends the main answer, Ctrl+Shift+Alt+L asks for Later. A swipe in either direction asks for Later.
- Demo: Try the Demo, the showcase and the test demo play a `work:3` host. UI tests pick the mode with `-mode inbox|tasks|sessions` on iOS and `--es mode inbox|tasks|sessions` on Android.

### tmux

- The sidebar shows the Inbox section above the session tree: the count and the first open items. `Enter` on one opens it in `prefix + T`.
- `prefix + T` shows the list with the selected item below it: its question, summary, numbered evidence and keys.
- Keys: `a` the item's main answer (Approve, Accept, Retry or Done), `r` Reject or Redo, `c` Change course, `p` Keep open, `x` Drop, `n` Dismiss, `l` Later, `1`–`9` open evidence (the diff first) or choose a question's option, `Enter` or `o` open the session. `space` and `b` scroll the item by a page, `J` and `K` by a line, and `v` shows it whole in the pager. Redo and Change course ask for the text. Only the keys the item takes are listed.

## Tasks and Sessions modes

- Tasks mode lists Routines first, then every Task grouped by what it needs: Needs attention, Ready for review, Running, No agent, Closed (`protocol/fixtures/inbox/groups.jsonl`). The detail is the Task's detail. A Routine's detail shows its workspace, trigger, enabled flag, worker, project, next run, last success, last failure, a running run with Open Terminal, and Run now. Run now on a bridge without the daemon shows the host's message.
- Sessions mode lists only sessions with no Task association, grouped by repo, with each repo's New Task and Ask (ADR 0025, 0031). The session switcher searches every session. A session's detail is its pane with the Chat / Agent / Shell switch; a Sessions row opens the session's last view.
- Settings holds the hosts (connect, add, edit, delete, reorder, Disconnect), Try the Demo, Leave Demo and the version. A status row under the mode control shows the connection and opens Settings. In the demo it reads "Demo" with Leave Demo.
- ADR 0033 slice I4 removed the Task's Inbox queue (ADR 0027 §4): Move down, the record's `inbox` field, `task-inbox` and the local Snooze. Later exists only on an Inbox item (`update-inbox` `later`, at most 366 days ahead).


Association is the classification boundary. Manually launched AI remains manual; typing by hand inside a Task does not change its classification. Manual approval waits can show a Sessions badge without generating Inbox items. Managed work needing human attention returns to Inbox as a reference to the same Task. Routine output enters Inbox as briefings/proposals, while the collector's execution remains under Routine history. No automatic Task creation or session reclassification is implied.

The domain model is defined in [Inbox, Tasks and Sessions](work-model.md). Session is an execution location that can host manual or managed work; only the default manual-session screen is filtered. Task/run identity survives human versus AI input changes. Recreating a session for an existing no-session Task and adopting manual work into a Task are separate future host operations.
