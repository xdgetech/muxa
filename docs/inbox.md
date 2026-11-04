# Inbox UI specification

The target is ADR 0033: the Inbox is a list of questions. Until its slices ship, the apps behave as in "Current behavior" below.

## Target (ADR 0033)

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

### Phone and tablet

- Phone: the list, then the item as a pushed screen. The answer buttons stay above the keyboard.
- Tablet: the list beside the item. Answering opens the next item.
- Keys: Command+Up/Down (Ctrl+Shift on Android) move between items, Return opens, Command+Return sends the main answer.

### tmux

- The sidebar shows the Inbox section above the session tree: the count and the first items.
- `prefix + T` shows the whole list in a popup. `Enter` opens an item: its question, summary and attachment names.
- In an item: `a` Approve or Accept, `r` Reject or Redo, `c` Change course, `x` Drop, `l` Later, `o` open the session, `d` open the diff in a pager, `1`–`9` open an attachment. Redo and Change course ask for the text.

## Current behavior (until ADR 0033 ships)

ADR 0023 defines the shared rules. Slice 1 provides navigation; direct actions remain in the existing Chat and Task detail screens.

### Session rows

Task sessions show the Task title, Inbox group and reported age on the first line; session name, branch and loop badge below. Closed records show Closed. Missing ages are omitted. Manual sessions keep their existing presentation. Search matches Task title as well as existing session fields.

### Inbox

A Triage/List control retains the existing grouped List and detail. Open Triage when its visible queue is nonempty, otherwise List. Triage shows position, group, Task title, workspace/project, session/branch when present, reported agent state and loop facts. Use “Ready for review” for both idle and done; do not infer that idle means a completed turn.

| Need | Text | Navigation |
|---|---|---|
| answer | Waits for your answer | Open Chat when supported, Open Terminal |
| agent-error | The agent stopped with an error | Open Terminal, Task detail |
| loop | Loop paused / Loop stopped: reason | Check log when available, Task detail, Open Session |
| review | Ready for review | Changes when supported, TASK.md, Check log when available, Task detail, Open Session |
| no-agent | No agent state is available | Open Terminal when a pane exists, Task detail |
| no-session | No session runs this Task | Task detail |

Only enable links with a supported capability and a known target. With no chat support, an answer card offers Open Terminal. Show secondary loop facts even when the primary need is answer or agent-error. The detail retains existing lifecycle actions, their confirmations and errors.

Phone: bottom bar Open, Snooze, Next. Tablet: card with the same controls, Previous and next two items. Swiping left moves to Next. At the end show “Nothing else needs you”, skipped/snoozed counts, Start over and Show List. Do not wrap automatically. Show snoozed restores local hidden items; List always includes them.

Reader opens over Inbox and returns to the same Task when it still exists. Opening Chat or Terminal uses the existing session navigation; Inbox never starts a chat behind the pane screen. Disconnect clears snoozes and disables stale-target links.

### Sidebar and initial navigation

Show up to three queue items with need labels, Triage all (count), and New Task. If empty, show Nothing needs you and All Tasks (List). Follow ADR 0023's once-per-connection phone routing after both lists arrive; tablet routing remains unchanged.

### Slice 1 keyboard controls

| Action | iPad | Android |
|---|---|---|
| Inbox | Command+T | Ctrl+Shift+T |
| Triage/List | Command+L | Ctrl+Shift+L |
| Next/Previous | Command+Down/Up | Ctrl+Shift+Down/Up |
| Open session | Command+O | Ctrl+Shift+O |
| Snooze | Command+S | Ctrl+Shift+S |

Scope shortcuts to the visible screen and suspend pane navigation chords while Inbox owns focus. Approval, denial and follow-up shortcuts remain in Chat. Slice 2's inline actions and desktop summary are proposed in ADR 0023, with no UI implementation yet.

### Next product direction: list-first processing

The developer wants an ordered actionable list with Act, Done, Move down and Later. Proposed ADR 0025 defines host-owned cross-device order and deferral, plus briefings/proposed work from scheduled project Routines. Move down keeps an item visible at the tail; Later has a return time. This is a proposed successor to slice 1, not an implemented change to current local Snooze or accepted queue sorting. All Tasks retains the grouped maintenance view.

#### Shared Move down and Later (host and CLI)

ADR 0027 §4 is implemented on the host and in the CLI. A Task record has an optional `inbox {order, until, since}`, written by `update-task` (capability `task-inbox`):

- Move down puts the Task at the end of the queue. `order` is the host's clock in milliseconds, so the latest Move down is last. The status does not change.
- Later hides the Task until `until` (UTC). Then it returns to its place.
- A new need surfaces again. When the believed agent state, or a loop that needs the user, began after `since`, the client ignores `order` and `until`.

The shared queue code in `muxa-cli`, MuxaKit and the Android `:session` module follows `protocol/fixtures/inbox/queue.jsonl`. The apps' queue applies the host's fields. Both apps offer Move down and Later (below).

#### The Inbox list (iOS and Android)

Both apps show Inbox as one ordered list (ADR 0025 "List first"). Triage, Next and Previous are gone there. The list holds, in this order:

1. Task needs without a Move down, in the queue order above.
2. Routine runs whose agent waits on the user: `running.task` matches a session's `taskId` (ADR 0026 section 3).
3. Stored briefings and proposals that are open or back from Later, newest first.
4. Tasks moved down, by `order`. So Move down puts a Task at the end of the whole list.

| Row | Act (tap) | Menu |
|---|---|---|
| Task | Task detail | Move down, Later, Mark Done |
| Briefing | its text and links | Done (dismiss), Later |
| Proposal | its text, action, target and content | Approve…, Dismiss, Later |
| Routine run | its terminal | Routine |

- Later offers In 1 hour, Tomorrow 9:00 and Pick a time. The host refuses a time in the past or more than 366 days ahead.
- Approve shows the action, the target and the content, and asks again. The host creates one Task, also when asked twice; the app opens it.
- Stored entries have no Move down on the wire, so their menu has none.
- A host without `task-inbox` keeps the local Snooze and Show snoozed. A host without `work:1` shows no entries and no Routines.
- Keys: Command+Up/Down focus a row, Command+O opens it, Command+Shift+Down moves it down, Command+Shift+L asks for Later, Command+L switches to All Tasks, Command+S snoozes on an older host.
- Android differs in two ways. Its keys are Ctrl+Shift chords, because plain Ctrl belongs to the shell: Ctrl+Shift+Up/Down, O, L and S, and Ctrl+Shift+Alt+Down and Ctrl+Shift+Alt+L for Move down and Later. A swipe right on a row asks for Later and a swipe left moves a Task down, where iOS shows both under one trailing swipe.

All Tasks keeps the grouped list and starts with Routines. A Routine's detail shows its workspace, trigger, enabled flag, worker, project, next run, last success, last failure, a running run with Open Terminal, and Run now. Run now on a bridge without the daemon shows the host's message.

#### Separate destinations

The proposed UI has three logical destinations: Inbox for actionable ToDos/briefings/proposals, All Tasks for running work and history, and Sessions for manual tmux work. All Tasks may be a link inside Inbox rather than a top-level tab; exact navigation placement remains a UI decision. Sessions lists only sessions with no Task association; it has no All Sessions filter (ADR 0027). The session switcher searches every session. Open Terminal on a Task returns to that Task's pane.

Association is the classification boundary. Manually launched AI remains manual; typing by hand inside a Task does not change its classification. Manual approval waits can show a Sessions badge without generating Inbox items. Managed work needing human attention returns to Inbox as a reference to the same Task. Routine output enters Inbox as briefings/proposals, while the collector's execution remains under Routine history. No automatic Task creation or session reclassification is implied.

The domain model is defined in [Inbox, Tasks and Sessions](work-model.md). Session is an execution location that can host manual or managed work; only the default manual-session screen is filtered. Task/run identity survives human versus AI input changes. Recreating a session for an existing no-session Task and adopting manual work into a Task are separate future host operations.
