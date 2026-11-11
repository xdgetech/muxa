# 0059: Today rows say what to do

## Status

Accepted on 2026-11-11. It amends the presentation of ADR 0049 §5 (a row shows its title, why, minutes and next step) and ADR 0057 §2 (the open rows, Next). It renames Plan Now to Re-rank in muxa for tmux and `muxa-cli`'s help. Hosts advertise `work:17`. muxa for tmux and the Android client are built; the iOS client is a follow-up (§7).

## Context

- The user: "I still don't understand on each today section, what I need to do, what I can do."
- A Today row shows its target's title, minutes and a mark. What to do is in the plan's `next`, below the why, in the preview. The keys are the target item's, listed last, with no word for the main one.
- Each target kind already has one main thing to do: a Task's `review` is to review its changes, an `ask` to allow or deny, a `result` (ADR 0058) to read. The row does not say which.
- Every item has a way to send text: `reply` to the Task's agent or the staff member, `ask-agent` to the workspace's Ask session. A Work item has none from Today. The secretary (ADR 0054) can take it, but only when the user opens the chat and types the context again.
- Plan Now re-ranks the queue from what is open now. Each one is a run of the overall workspace's `max_runs_per_day` (ADR 0057). The client cannot tell how many are left.

## Decision

### 1. A row names its action

A row is `<verb> · <title>  <minutes>`, for example `▶ 1. Review changes · improve ordoq jibai state  20–40 min`. The verb comes from the target's kind:

| Target | Verb |
|---|---|
| `review` | Review changes |
| `ask` | Allow or deny |
| `result` | Read result |
| `write` | Check text to send |
| `briefing` | Decide, or Read when its question does not end in `?` |
| `failure` | Fix |
| `memory` | Check memory note |
| `proposal` | Approve or reject |
| `sort` | Sort capture |
| `delegate` | Approve request |
| `hypothesis` | Confirm or reject |
| `handoff` | Approve hand-off |
| Work item | Start |
| anything else, or an item no longer listed | Open |

A row taken off Today keeps the verb on its one line.

### 2. The preview leads with what to do

```
Do: <the row's next>
Enter  <the primary action>
s      <Send to whom>…
also:  <the target's other keys>
plan:  K/J move   x not today   t Re-rank (N of M runs left today)
```

Why, Time, Basis and Would help follow. "Do" replaces "Next". The `plan` line is dimmed: it changes the queue, not the target. A row whose target closed shows Do and the plan line only.

### 3. Enter is the primary action

- A `review` or `ask` with a session opens the Task's session, as Enter on the item does.
- Any other item, and a Work item, moves to the target's row. There its keys and evidence are.
- `o` still moves to the target's row, also from a `review` or `ask`.

### 4. `s` sends, and names who gets it

| Target | What `s` does | Label |
|---|---|---|
| `review`, `failure` that offer `reply` | `reply` to the Task's agent | Send to the Task's agent… |
| `ask` | `reply`: deny, then send | Deny & send to the Task's agent… |
| `result` | `reply`: the member who wrote it goes on (ADR 0058) | Send to `<worker>`… |
| `write` | `reply`: Revise (ADR 0058) | Revise: send to `<worker>`… |
| `briefing`, and any item that offers `ask-agent` | `ask-agent` (ADR 0041) | Send to the `<workspace>` Ask session, with this briefing… |
| Work item | the secretary, with the Work item (below) | Send to the secretary (`<overall>`), with this Work item… |
| an item with neither answer | nothing | says so, and points to the keys below |

`<worker>` is the result's or the write's `worker`, else "the assistant". The Task's agent has no name on the wire; the row's title names the Task.

A Work item's Send needs the host. `start-session {workspace, root: true, text, work?}` (level 17) opens the root Ask session, as level 15 does, then types one prompt into its agent with `ask-agent`'s rules: the Work item (id, state, workspace, title, newest result's summary, and `muxa-cli work show <id>`), then `My request: <text>`. On Today, `workspace` is `assistant.overall`: the secretary. `muxa-cli ask --root --workspace W --text T [--work WORK]` sends it. `s` on a Work item in the Work section does the same.

### 5. Re-rank, with the runs left

Plan Now is **Re-rank** in muxa for tmux and in `muxa-cli plan now`'s help: "re-rank the queue from what's open now". `inbox-list`'s `assistant` gains `overallRuns {left, max}` (level 17): `max` is the overall workspace's `max_runs_per_day`, and `left` the smaller of what it and the host's `[assistant] max_runs_per_day` leave today, counting the plans being made. The plan line and the Today heading show "(2 of 3 runs left today)". The request stays `plan-now`, and the command stays `muxa-cli plan now`.

### 6. The secretary says the verb and the target

When the secretary gives the next row (ADR 0057 §5), it says the verb, the target, and how to do it: "Review the jibai PR: Enter on Today row 1, or say 'open'".

### 7. Clients: a follow-up

The iOS and Android apps show the same layout in a follow-up:

1. Row: the verb before the title, from the same table (§1).
2. Row detail: Do (the plan's `next`) first; the primary action as the main button (§3); a Send button that names the receiver (§4).
3. Work item rows at level 17: Send to the secretary sends `start-session` with `root`, `text` and `work` for `assistant.overall`, then opens the secretary's Chat.
4. Plan Now becomes Re-rank, with "N of M runs left today" from `assistant.overallRuns`.
5. Types: `start-session`'s `text` and `work`, `AssistantInfo.overallRuns {left, max}`, and `workToday = 17`, in Swift and Kotlin. Their fixture tests read `protocol/fixtures/today-actions/`.

## Consequences

- Each row says what to do before the user opens it, and the preview says how.
- The verbs are the client's. A new item kind is "Open" until a client names it. A briefing's verb is a guess from its question; the plan's `next` stays the exact words.
- The sidebar tree has no Today rows, so it does not change.
- A level 16 host ignores `text` and `work` and only opens the session. The CLI and muxa for tmux send them only at level 17.
- Alternatives not taken:
  - The plan file carries a verb per row: the agent would name actions the client cannot do, and an old plan has none.
  - A new request `ask-secretary`: a second way into the root Ask session next to `start-session` with `root`.
  - Typing the Work item into the secretary's pane from muxa for tmux: it skips the host's checks for a menu or a person typing, and the apps could not do it.

## Addendum (2026-11-11): a Work item's Enter is a menu

The user: "for the work, it says there is no session and i cannot do anything". Enter on a Work row in muxa for tmux took the item path and said "This item has no session to open". §3 only moved the cursor from Today to the Work row, so Enter there had nothing to do. tmux could not start a Task for a Work item either, although `create-task` takes `work` (ADR 0048).

This amends §3 for muxa for tmux:

1. Enter on a Work row, in the Work section or on Today, opens a menu built from the item. Each line says what it does:
   - Open its Task: one line per linked Task (`tasks[]`) with a live session, from `muxa-cli tasks --json`.
   - Read the result: one line per open `result` item whose `result.work` is the item (ADR 0058). It moves to that item.
   - `n` Start a Task from it: the New Task form with the item's workspace and title. It sends `muxa-cli task new --work WORK`, so the Task is linked to the item.
   - `a` Ask the assistant, `s` Send to the secretary (level 17), `d` Done, `l` Later, `P` Pin or Unpin.
2. The keys also work on the row without the menu. The row's preview leads with `Enter  Actions` and lists `n Start a Task`.
3. `a` asks what to find out and the scope (research, compare or draft) after the staff member, and sends them as `--purpose -`. Before, it sent no purpose, and the host ran the request's last purpose again as a new version.

The apps keep §3 until their follow-up (§7).
