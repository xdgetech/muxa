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
- A protected item shows a lock (`lock.fill`, "Protected" for VoiceOver and TalkBack) after its age (ADR 0046).
- **Delete Old** (`trash`), in the list's toolbar menu: **Older than 1 day**, **3 days** or **5 days**. A confirmation names the count: "Delete 12 items older than 3 days? Protected items stay." with a destructive **Delete** and **Cancel**. The count is the listed items older than that, not protected. With none, it says "No unprotected items are older than 3 days." with only **OK**. After the delete the list loads again. A host below `work:8` shows no Delete Old.

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
4. The top bar is triage, the same on every item (ADR 0040 §3): **Later** (gray, the "Play Last" symbol) moves the item to the bottom; **✓** (green) closes only the item; **…** holds **Complete Task**, **Drop Task**, **Protect** or **Unprotect** (`lock` / `lock.open`, with `work:8`, ADR 0046) and **Open Session**. A protected item still takes ✓, Later and every answer; only Delete Old keeps it.
5. The bottom answers the agent: its decisions, then one reply box with a send arrow.

| Kind | Bottom | … |
|---|---|---|
| `ask` | Allow, Deny; a question shows its options; reply box: Deny & Send | Protect, Open Session |
| `review` | reply box: Send | Complete Task, Protect, Open Session |
| `failure` | Retry; reply box: Send. An assistant run's: Retry; reply box: Ask Agent | Drop Task, Protect, Open Session. An assistant run's: Protect |
| `proposal` | Approve, Reject; reply box: Ask Agent | Protect |
| `briefing` | reply box: Ask Agent | Protect |
| `result` | Hand off to `<worker>` (with a hand-off); reply box: Send | Protect |

The reply box sends **Send** on an item with a session: the text goes to its agent as it is. On one without a session it sends **Ask Agent**: the item and the text go to the workspace's Ask session, `ask-<workspace>`, which the host starts on the first request; the item stays open and the session's Chat opens (ADR 0041). The placeholder says where the text goes. Complete Task and Drop Task ask for no confirmation, because they only set the Task's status; Approve on a proposal still shows the action, target and content first. An answer returns to the list.

### Phone and tablet (ADR 0035)

- The sidebar shows one mode: **Inbox**, **Tasks** or **Sessions**. Inbox mode is the host's full list of items, in the host's order, with no fold. The Inbox count is on the mode control.
- Tablet: the list is the sidebar and the selected item is the detail. At connect, and on a switch to Inbox mode, the first item is selected (which marks it read). After an answer the next item opens.
- Phone: the list is the first screen. The item is pushed; an answer returns to the list. Back goes pane → item → list.
- The item's session opens from its source line or Open Session. On a phone it pushes over the item; on a tablet the pane fills the detail and the sidebar collapses. An item never opens the Shell first.
- The summary is Markdown (ADR 0033 §4a.5). Its `http` and `https` links open in the browser. A link or a file name that matches an attachment opens that attachment in the Reader.
- The attachments are a list under the summary: Changes with the file and line counts, then each doc and image by file name and folder, then the links by title. Evidence opens in the Reader and returns to the item.
- Labels name the result (ADR 0033 §4a.2, ADR 0040). `accept` is **Complete Task** and `drop` is **Drop Task**. `keep-open` and `dismiss` are **✓**, named "Done" for VoiceOver and TalkBack. `later` is the "Play Last" symbol, named "Later". `reply` is **Send**, or **Deny & Send** on an `ask`. On an `ask`, `approve` is **Allow** and `reject` is **Deny**. A proposal keeps **Approve** and **Reject**. On a `result`, `approve` is **Hand off to `<worker>`**; on a `write`, `reply` is **Revise** (ADR 0058). On a `result` that proposes a Task and on a `new-task` item, `approve` or `start-task` is **Start Task**; on a plan `ask`, `approve` is **Approve plan** and `reject` **Keep planning** (ADR 0060).
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
- The keys follow the apps' item screen (ADR 0040), on two lines. Triage: `l` Later (moves the item down, with no time), `d` ✓ Done (only the item), `c` Complete Task, `x` Drop Task, `p` Protect or Unprotect (`work:8`), `o` Open Session. Answers: `a` the first decision (Allow, Approve or Retry; never Complete Task), `r` Deny or Reject, `1`–`9` a question's option, `s` the reply box (Send with a session, Deny & Send on a permission, Ask Agent without a session; it asks for the text). Otherwise `1`–`9` open evidence (the diff first). `y` copies one of the item's URLs, from its links or its text, to the clipboard and opens it on this machine. `Enter` also opens the session. `space` and `b` scroll the item by a page, `J` and `K` by a line, and `v` shows it whole in the pager. Only the keys the item takes are listed.
- `D` is Delete Old (`work:8`): it asks for 1, 3 or 5 days, then confirms with the count, as the apps do. A row and an item show "protected".
- Labels are the apps' labels (ADR 0033 §4a, ADR 0040): `accept` is "Complete Task", `drop` "Drop Task", `keep-open` and `dismiss` "✓ Done", `later` "Later", `ask-agent` "Ask Agent", `reply` is "Send" ("Deny & Send" on an `ask`), and on an `ask` `approve` is "Allow" and `reject` "Deny". A proposal keeps Approve and Reject.

## Work and Unsorted (ADR 0048)

With `work:9` the Inbox has three sections, in this order. An older host shows Decisions only.

```
Decisions
● Approve terraform apply
  lisk-infra · 4m
Work
  Compare hosting plans
  open · Company
  Result ready
  Reduce GA cost
  waiting on decision · Lisk
Unsorted
  call the bank about the card
  2h · Personal
```

1. **Decisions**: the Inbox items above. The Inbox count on the mode control counts these only. Work items and Captures are not counted.
2. **Work**: open Work items in the host's order; items moved down with Later come last (ADR 0040). A row shows the title, then the state, the workspace and "later". A Work item whose Task has an open item shows "waiting on decision", with no answer of its own: the decision is in Decisions.
3. **Unsorted**: Captures not sorted yet, newest first. Captures still on the device come first, marked "Not sent", and are sent again on connect.

- Swipe on a Work row: right is **Complete**, left is **Later** (moves it to the bottom, never hides it).
- Swipe on an Unsorted row: right makes a **Work Item**, left keeps it as a **Note**. A "Not sent" row swipes left to **Delete**.
- A Work row with an assistant request shows its newest delivery: `pending`, `received`, `processing`, `unconfirmed`, `failed`, `timed-out`, `cancel-requested` or `cancelled` (ADR 0048 §6). These are derived from the run on every list; `failed` and `timed-out` show as needing attention.
- A `failed` request step, and a failed plan, also make one `failure` item in Decisions, with the reason as its question (ADR 0048, addendum 2026-11-11). For example: "Claude Code has not trusted `<workspace root>`. Open claude there once and accept." It has no Task and no session: its answers are Retry, ✓, Ask Agent and Later. Retry (ADR 0061) asks the request again as a new version, runs the plan again now, or lets the daemon start the secretary's chat again with a fresh restart cap. A budget can refuse it; the item then stays open. A second Retry starts nothing.
- **Result ready** replaces the delivery when the newest Result was not shown on this device yet, on a host below `work:16` only. The device keeps the ids and versions it showed, never the Result's text. Opening the Work item clears it. With `work:16` a new Result is an unread `result` item in Decisions instead (ADR 0058).
- The Work detail shows the Captures' full texts, each request with its delivery and Results, Cancel, Ask Again, and Open Session while the run lives. Result files are paths; the Reader cannot open them yet.
- `+` opens Capture: text, an optional workspace, **Save**, **Ask Assistant** and "Start a Task instead…". A host below `work:9` keeps New Task.
- muxa for tmux: `prefix + T` lists Work rows. `Enter` opens a menu: Open its Task (a linked Task with a live session), Read the result (an open `result` item of it), `n` Start a Task from it (the New Task form with its workspace and title, `muxa-cli task new --work WORK`), `a` ask, `s` Send to the secretary, `d` complete, `l` Later and `P` Pin. The same keys work on the row. `a` asks what to find out and the scope, and sends them as the request's purpose.

## Today, proposals and pins (ADR 0049)

With `work:10` and an overall assistant, a **Today** section comes above Decisions. It shows the newest plan with a result.

1. **Rows**: a queue of up to 10, most important first (ADR 0057). Each row shows its Work item's or Inbox item's title, the why, the minutes and the next step. A row of a Work item whose request has no answer yet, or whose newest Result is older than the item, says so. Tapping a row opens its Work item or Inbox item. A row leaves Today once its target is closed: an Inbox item answered or resolved, a Work item done or withdrawn, or a Work item with a Task started for it (`targetState` `started`; the Task's own items follow).
   - The first open row says **Next**. Work down the queue in that order.
   - A row whose item you closed (done, withdrawn, answered, resolved, or gone from the host) is hidden.
   - When every row is closed or Not today, the section says "All done", and the host makes a new plan by itself. It makes one such plan per emptied plan, within the overall assistant's budgets. A reached budget shows as a failed plan with its reason.
2. **Why** opens the row's basis (due, dependency, goal, time or manual, with its source and time) and what is missing.
3. **Due soon**: one line under the rows, "Due soon, moved down by you: …", for items you moved down with Later that are due within 24 hours. They are never rows. The line names each by its title, and leaves out one you completed or withdrew since the plan.
4. **New**: the header shows New when a plan arrived that this device has not shown yet. A new plan replaces the rows but never changes the selection or the scroll.
5. **Plan Now** asks the overall assistant for a plan now. While it runs, the section says "Making a plan…". A failed or cancelled newest plan shows the host's `reason` as it is, above the last good rows. With no plan yet, or an empty one, the section says so.
6. **Talk to Assistant** (`work:15`, ADR 0054) opens the overall assistant's Chat: its root Ask session, started on the first tap and the same one after. It is also in the `+` sheet while the sheet is empty. iOS shows it as a row above Plan Now; Android as **Talk** in the header. muxa for tmux: `prefix + A`, the overall workspace, then its only row.

- iOS: a tap on Plan Now sets no time limit. A long press offers "I Have 30 minutes", "1 hour" or "2 hours".
- Android: **Re-rank** (ADR 0059 §5) opens a dialog with an optional "Minutes you have" field (1 to 1440). With `work:17`, a line under the header and the dialog say "2 of 3 runs left today".
- The apps do not show `changed` (a row new or moved since the previous plan). muxa for tmux and `muxa-cli plan` do.

The overall assistant's proposals are Decisions items of kind `sort` (sort an unsorted Capture) and `delegate` (ask a workspace's assistant to work on a Work item). Their answers are **Approve**, **Reject** and **Later**. Approve runs the action once; approving twice makes nothing new. A `delegate` the host sent itself (`overall_requests = "auto"`) is already answered, and its summary says so.

Pins and due dates live on Work items:

- Work detail has **Pin** / **Unpin** and **Due**. A pinned item comes first in Work, in pin order, and fills the first Today rows. Pin clears Later; Later, Complete, Withdraw and Merge clear the pin.
- A Work row shows "pinned" and "due <date>" after its state.
- Work detail shows a Result's proposed notes (`WORKSPACE.md` for the lead, a member's `NOTES.md`) in full, with **Update Notes**. The apps keep no copy of the live file, so they show the whole text, not a diff. A changed live file is `conflict`: ask again.

muxa for tmux (`prefix + T`): Today comes first, with `new` or `moved` on a row. `o` on a row goes to its item in the list. `t` is Re-rank: it asks for the minutes first (`Enter` gives no limit). `P` on a Work item pins or unpins it. `sort` and `delegate` items take `a` Approve, `r` Reject and `l` Later.

### What each Today row asks (ADR 0059)

A row names what to do before its title: `▶ 1. Review changes · improve ordoq jibai state  20–40 min`. The verb comes from the target: Review changes (a Task's review), Allow or deny (a Task's menu), Read result (a staff member's result), Check text to send (a write), Decide or Read (a briefing), Fix (a failure), Check memory note, and Start (a Work item). An item the client does not know is Open.

The row's preview, in muxa for tmux:

```
Do: <what the plan says to do next>
Enter  Open the Task (review)
s      Send to the Task's agent…
also:  c Complete Task   d ✓ Done   l Later   o Go to the item
plan:  K/J move   x not today   t Re-rank (2 of 3 runs left today)
```

1. **Enter** is the main action. A Task's review or menu opens its session. A Work item opens its menu (the Work section, above). Any other item goes to its row in the list.
2. **s** sends text, and names who gets it: the Task's agent (`reply`; on a menu, it denies first), the staff member who wrote a result, the writer of a draft (Revise), the workspace's Ask session with a briefing (`ask-agent`), or, for a Work item with `work:17`, the secretary with the Work item (`muxa-cli ask --root --workspace <overall> --work WORK --text T`).
3. **also** lists the target's other keys.
4. **plan** changes the queue, not the target. **Re-rank** (`t`, `muxa-cli plan now`) ranks the queue again from what is open now. Each one is a run of the overall workspace's `max_runs_per_day`. With `work:17`, the host sends `assistant.overallRuns`, and the line says how many are left today.

The iPad and iPhone app, with `work:17`: a row reads `Review changes · <title>`, then why, **Do:** and the minutes. Under it, the main button (Open Session for a Task's review or menu, else Open Result, Open Work Item…) and **Send…**, whose prompt names who gets the text. Tapping the row still opens its item. Work detail has **Send to the Secretary…**. Plan Now is **Re-rank**, with "2 of 3 runs left today".

Android (ADR 0059 §7): a row reads `<verb> · <title>`. A tap is the main action. **Details** opens Do, the main action's button, Send with its receiver ("Send to the secretary (secretary), with this Work item" at `work:17`), then Why, Time, Basis and Would help. A row whose target takes no text says so instead of Send. Work detail has **Send to the Secretary** at `work:17`. A Work row's tap opens Work detail; Android has no Work menu.

## Policies, Today order and hypotheses (ADR 0050)

With `work:11`, the user can correct the newest plan and tell the overall assistant what matters, in their own words.

1. **Reorder Today.** iOS: drag the rows in edit mode, or Move Up / Move Down in a row's context menu. Android: Move Up / Move Down in a row's menu. The order is kept on the plan. The next plan does not start from it.
2. **Not Today** takes a row off Today, with an optional reason. The row stays as one line. A reason is never required.
3. **Why** lists the policies, hypotheses and corrections a row cites, by their text. A hypothesis or a correction has **Forget**. A record that is gone shows "forgotten".
4. **Hypothesis items** in Decisions are the overall assistant's guesses at a preference, "Based on 3 corrections". **Confirm** keeps the text as a policy. **Reject** takes an optional reason and tells the assistant not to use it again. **Later** moves it down.
5. **Policies** opens from the Today header and from Settings, on both clients. It lists the active policies with their scope and end, then Expired, Hypotheses and Recent Corrections. **Add Policy** takes the text, a scope (all or one workspace, never the overall one) and an end: this week, a date, or none. A policy has **Edit** and **Delete**; a hypothesis **Confirm**, **Reject** and **Forget**; a correction **Forget**.

Forget removes a record from what the assistant reads next, with the hypotheses based on it. Plans made before keep their reasons. Later and Not Today never tell the assistant that an item is unimportant.

muxa for tmux (`prefix + T`): `K` and `J` move a Today row up and down. `x` takes a row off Today and asks for a reason (`Enter` gives none). `💡` items are hypotheses: `a` Confirm, `r` Reject with an optional reason, `l` Later. `Y` opens the Policies list: `n` adds a policy, `e` edits, `d` deletes, `f` forgets a hypothesis or a correction. Re-rank is `t`, because `p` is Protect.

## Staff and hand-offs (ADR 0051)

With `work:12`, a workspace's assistant can be a staff of several members. Its Work items show the team's steps.

1. **Ask Assistant** has a staff picker when the workspace has more than one staff member. The default is "Let `<lead>` decide". Each member shows its description as its subtitle.
2. **Work detail** shows a request's steps as a chain of rows, "`lisk-research` → `lisk-delivery` → `lisk-meetings`". Each row has its delivery, its Result and Update Notes. A hand-off that waits for approval is the next row, greyed, with Approve and Reject.
3. **Hand-off items** in Decisions have kind `handoff`, on a host below `work:16`. The title names both staff members; the summary is the hand-off's purpose. **Approve** starts the next step, once. **Reject** ends the chain with the Results it has. **Later** moves it down. A budget reached keeps the item open with the limit. With `work:16` the step's `result` item offers the hand-off instead (below).
4. **Today**: a `delegate` row names the staff member when the plan picked one.

A host below `work:12` shows a request's newest step as the request. muxa for tmux (`prefix + T`): `a` on a Work row asks for the staff member (`Enter` for the lead), then for the question. `handoff` items take `a` Approve, `r` Reject and `l` Later.

## Write proposals (ADR 0052)

With `work:13`, a Result may propose writes outside the host, such as a Slack post. Each write is one Decisions item of kind `write`. The workspace's writer performs it only after the user approves its exact text.

1. **The item.** The title is the action, "Post to #release". The question names the staff member and the target: "`lisk-delivery` proposes to write to `slack:#release`". The summary is the reason.
2. **The content** is shown whole, above Approve, Reject and Later: plain text in a fixed-width font, no rendered Markdown, no active links. A write from an older result version says "from an older result".
3. **Approve** shows the target once more to confirm. It sends the write's hash, so the host performs only the text the user saw. It starts one Task of the writer; the item says "Task started", then the Task's own state. Approving twice starts one Task. A refusal (no writer, `max_writes_per_day` reached, the writer cannot start) keeps the item open with the reason.
4. **Reject** takes an optional reason and starts nothing. **Later** moves the item down.
5. **Revise** (`work:16`, ADR 0058): a reply box, "Ask `<worker>` for a new draft…". The member writes a new draft as a new request version, and the item closes. Its new Result proposes the new text as a new item.
6. **Work detail** lists each Result's writes with their state: proposed, rejected, or the Task with its state and Open Session.
7. **Staff settings** list the writer under the staff, marked "writes", with its description.

The writer's Task reports what it did in its own `review` item. muxa does not check the target system: the user sees the write there. A client below `work:13` shows a `write` item as an unknown kind, without Approve. It cannot approve a write: approve without the hash is refused.

muxa for tmux (`prefix + T`): the preview shows a `write` item's target and content whole. `a` approves after the target is shown once more and confirmed with `y`. `r` rejects, `l` moves it down, `s` revises (`work:16`). `muxa-cli inbox show ITEM` prints the item with its content, and `muxa-cli assist writes WORKSPACE` lists a workspace's writes.

## Results to review (ADR 0058)

With `work:16`, every Result of an assistant request makes one Decisions item of kind `result`, reviewed like a Task's `review` item.

```
┌← Inbox     Result     ⤓  ✓  … ┐
│ lisk-strategist asks           │
│ lisk-architect to: Check …     │
│ Decide the Q1 roadmap · lisk   │
│                                │
│ Option B fits the hiring plan. │
│ Findings                       │
│ · Option B needs one hire fewer│
│ Open Questions · Next Steps    │
│ Files                          │
│                                │
│ [ Hand off to lisk-architect ] │
│ ┌ Tell lisk-strategist what…(↑)┐│
└────────────────────────────────┘
```

1. **The item.** The title is the Work item's. The question is "`<worker>` finished: `<summary's first line>`", or "`<worker>` asks `<next>` to: `<purpose>`" with a hand-off. The summary is the Result's summary, as Markdown. Under it: Findings (each finding's first line), Open Questions, Next Steps and Files (paths, as text), then the findings' links. The user reviews the Result without opening the Work item.
2. **Hand off to `<worker>`** shows only when the step proposes a hand-off. It starts the next step, once, as Approve on a `handoff` item did.
3. **Send**: the reply box, "Tell `<worker>` what to do next…". The same member goes on, continuing from this Result, as a new version of the request. The older Results become stale; the item closes.
4. **Done** is ✓: the item closes, and the hand-off never starts. **Later** moves it down.
5. Only Results of the last 24 hours make an item. A newer version of the request resolves the item; a cancel leaves it open without its hand-off.
6. The item's unread dot is the "new result" mark. The Work row's **Result ready** shows only for a host below `work:16`, and muxa for tmux's Work rows carry no mark.

A client below `work:16` shows a `result` item as an unknown kind. Its answers are actions it knows (`approve`, `reply`, `dismiss`, `later`), so its buttons still work.

muxa for tmux (`prefix + T`): `📋` rows. The preview shows the Result's lists and the hand-off. `a` hands off, `s` asks for the text and sends it, `d` is ✓ Done, `l` is Later. CLI: `muxa-cli inbox answer ITEM handoff | send --text T | done`, `inbox answer ITEM revise --text T` on a `write` item, and `inbox show ITEM` prints the Result's lists.

## Proposed Tasks (ADR 0060)

With `work:18`, the assistant proposes a Task when the work needs code or other changes on this host. The user starts it from the Inbox, then reviews its plan before any edit.

```
┌← Inbox     Result     ⤓  ✓  … ┐
│ muxa-research proposes a Task: │
│ Fix the Inbox crash            │
│ Fix the Inbox crash · muxa     │
│                                │
│ The crash comes from an empty  │
│ summary.                       │
│ Proposed Task                  │
│ muxa · repo muxa               │
│ Guard the empty summary …      │
│ Starts in plan mode            │
│                                │
│ [ Start Task ]                 │
│ ┌ Tell muxa-research what… (↑)┐│
└────────────────────────────────┘
```

1. **On a `result` item.** A Result may propose one Task: a title, a goal, and a repo of the workspace or none. The item shows the version's proposal: the lead's, else the newest one. **Start Task** is `approve` when the item offers no hand-off. Beside **Hand off**, it is its own answer, `start-task`.
2. **The secretary's `new-task` item** (🛠). The secretary proposes with `muxa-cli task propose`. **Start Task** is `approve`. **Send** ("Tell the secretary what to change…") types the text into the secretary's conversation; the item stays open, and the secretary proposes again. **Done** closes it.
3. **Start Task** starts one Task, also when pressed twice: the workspace's default Task worker, linked to the Work item, in plan mode. The item closes and says "Started Task `<id>`: `<title>`"; **Open its Task** opens it. On a `result` item with a hand-off, the chain ends there.
4. **Plan mode.** Claude Code starts with `--permission-mode plan`, so it plans first. Its plan comes back as an `ask` item of the Task: **Approve plan** leaves plan mode with edits asked one by one. muxa presses that option only when the pane shows it; otherwise it says to approve in the terminal. **Keep planning** keeps it in plan mode, and **Keep planning & Send** also sends what to change. Codex has no plan mode in muxa: the goal asks it to plan first, and its plan arrives as a `review` item, answered with Send.
5. **Talking to the Task.** A Task's items keep Send to the Task's agent. The Work item's menu lists Open its Task, and Send to its Task's agent when the Task has an open item.

A client below `work:18` shows a `new-task` item as an unknown kind whose Approve, reply box, ✓ and Later work. It never sends `start-task`.

Android: the item shows a **Proposed Task** card (title, workspace and repo, goal, "Starts in plan mode"). **Start Task** is the main button; beside a hand-off, **Hand off to `<member>`** stays next to it. After Start Task the item screen says "Task started" with **Open its Task**. A plan item is titled Plan, with **Approve plan**, **Keep planning** and the reply box "Keep planning & send…". When Approve plan finds no option to press, a dialog says to approve in the terminal and offers **Open Session**.

muxa for tmux (`prefix + T`): `a` is Start Task, or `n` beside a hand-off; on a `🛠` item `s` goes to the secretary; on a plan item `a` approves the plan, `r` keeps planning, `s` keeps planning and sends; `o` or Enter on an item that started a Task opens it. CLI: `muxa-cli task propose`, `muxa-cli inbox answer ITEM start`.

## Workspace memory (ADR 0053)

With `work:14`, a Task that ends with text under "For the workspace" in its `TASK.md` proposes it for the workspace memory, `<root>/.muxa/WORKSPACE.md`. By default (`memory = "auto"`, ADR 0053 Revision 1) the host appends it at once and makes no item. Under `memory = "review"`, or when `auto` cannot append, each Task makes at most one Decisions item of kind `memory`; it stays open after the Task closed.

1. **The item.** The title is the Task's title. The question is "`<title>` proposes to add to the workspace memory". It links the Task.
2. **The text** is shown whole, above Approve, Reject and Later: plain text in a fixed-width font, no rendered Markdown, no active links. Approve names the workspace once more to confirm.
3. **Approve** sends the item's hash, so the host appends only the text the user saw. The host appends it under `## <title> (<task>, <YYYY-MM-DD>)`. Approving twice appends once. When the file would pass 16 KiB, approve is refused with the sizes and the item stays open: ask the lead to shorten the file, then approve again.
4. **No Approve** when the text is over 4 KiB or has control characters; the summary says why. Copy what you need by hand. **Reject** writes nothing. **Later** moves it down.
5. **`memory = "auto"`** appends at the Task's end without an item. A refused append makes the item, with the reason in its summary. **`memory = "off"`** proposes nothing.

A client below `work:14` shows a `memory` item as an unknown kind, without Approve.

muxa for tmux (`prefix + T`): `🧠` rows show the Task and the text whole in the preview. `a` approves after the workspace is shown once more and confirmed with `y`. `r` rejects, `l` moves it down. `muxa-cli inbox show ITEM` prints the text, and `muxa-cli memory WORKSPACE` prints the live file.

## Tasks and Sessions modes

- Tasks mode lists Routines first, then every Task grouped by what it needs: Needs attention, Ready for review, Running, No agent, Closed (`protocol/fixtures/inbox/groups.jsonl`). Closed lists a Task for a day after its last change; an item can still open an older one. A Task with a session opens its pane, as a Sessions row does; its detail is Task Info in the pane's More menu (ADR 0044). A Task without a session, or a closed one, shows its detail. A Routine's detail shows its workspace, trigger, enabled flag, worker, project, next run, last success, last failure, a running run with Open Terminal, and Run now. Run now on a bridge without the daemon shows the host's message.
- Sessions mode lists only sessions with no Task association, grouped by repo, with each repo's New Task and Ask (ADR 0025, 0031). The session switcher searches every session. A session's detail is its pane with the Chat / Agent / Shell switch; a Sessions row opens the session's last view.
- Settings holds the hosts (connect, add, edit, delete, reorder, Disconnect), Try the Demo, Leave Demo and the version. A status row under the mode control shows the connection and opens Settings. In the demo it reads "Demo" with Leave Demo.
- ADR 0033 slice I4 removed the Task's Inbox queue (ADR 0027 §4): Move down, the record's `inbox` field, `task-inbox` and the local Snooze. Later exists only on an Inbox item (`update-inbox` `later`). Since ADR 0040 it moves the item down instead of hiding it until a time.


Association is the classification boundary. Manually launched AI remains manual; typing by hand inside a Task does not change its classification. Manual approval waits can show a Sessions badge without generating Inbox items. Managed work needing human attention returns to Inbox as a reference to the same Task. Routine output enters Inbox as briefings/proposals, while the collector's execution remains under Routine history. No automatic Task creation or session reclassification is implied.

The domain model is defined in [Inbox, Tasks and Sessions](work-model.md). Session is an execution location that can host manual or managed work; only the default manual-session screen is filtered. Task/run identity survives human versus AI input changes. Recreating a session for an existing no-session Task and adopting manual work into a Task are separate future host operations.
