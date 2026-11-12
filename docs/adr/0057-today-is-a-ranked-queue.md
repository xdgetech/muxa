# 0057: Today is a ranked queue

## Status

Accepted on 2026-11-11. It amends ADR 0049 §5 (the plan's `top[]` holds at most 3 rows) and the "morning top 3" of the [specification](../personal-assistant.md) §7. It changes the 2026-11-11 addendum of ADR 0049 on closed rows: clients hide them instead of dimming them. No new `work` level.

## Context

- Today shows at most 3 rows of the newest plan with a result (ADR 0049 §5). The user resolves 3 rows in a few minutes. Then Today is a list of finished work until the next morning plan.
- Since the 2026-11-11 addendum of ADR 0049, a row whose target closed stays in place, dimmed, with how it ended. The user said the finished rows are noise.
- What the user wants: the assistant says what to look at next, in order of importance, and the user works down that order.

## Decision

### 1. A plan ranks up to 10 rows

`top[]` holds 0 to 10 rows. The plan brief asks the overall agent to rank every open Inbox item that needs the user and every open Work item, most important first, up to 10. A Routine `briefing` is a row only when it asks the user to act. Pinned Work items still fill the first rows, oldest pin first. Every other rule of ADR 0049 §5 stays: no Work item with `laterAt`, no closed item, and the host checks the file.

### 2. Clients show the open rows, and mark the first one Next

- A client shows only rows whose target is open: rows without `targetState`. A closed row is hidden, not dimmed. The host still sends `title` and `targetState` (ADR 0049 addendum, item 1).
- A row the user took off Today ("not today", ADR 0050) stays as one line, as before.
- The first open row that is not "not today" is **Next**: tmux marks it with `▶`; the apps show a Next label.
- Reorder and "not today" still act on every row of `top`, also hidden ones: `update-plan reorder` names each row once (ADR 0050 §2).

### 3. The host plans again when the queue is empty

When every row of the newest `resulted` plan is closed or "not today", and that plan had rows, the daemon starts one plan, as `plan-now` does.

1. It runs in the daemon's plan tick, after the schedule, and only when no plan is being made.
2. It is one refill per emptied plan. The host marks the emptied plan (`refilled` in the plan file's extra fields) before the run starts, so a failed or cancelled refill does not start another.
3. The overall workspace's budgets apply (`max_runs_per_day`, `max_open_runs`, and the host's). A reached budget keeps the refill as a `failed` plan with its `reason`, as a scheduled plan's (ADR 0049 §9). It never loops.
4. A plan with no rows never refills: it says there is nothing to rank.

### 4. Wire: a limit, not a level

`top` at most 10 changes a limit in `PROTOCOL.md`, not a type. Old clients decode any number of rows and lay them out in a list, so they show 10 rows; they still dim closed rows. A refill is a plan like any other. No `work` level is needed.

### 5. The secretary answers in queue order

The secretary's brief (ADR 0054) tells it: when the user asks what to do next, answer from the queue of `muxa-cli plan --json`, open rows only, one at a time. After the user resolves one, give the next. When the queue is empty or old, ask the user whether to run `muxa-cli plan now`.

## Consequences

- Today shows what is left, in order, with one row marked Next. The user does not scroll past finished work.
- A plan costs one overall run. A user who clears the queue several times a day spends more of `max_runs_per_day`. The budget stops it with a visible reason.
- A plan brief and a plan file grow with 10 rows. Both stay within their size limits: a row's texts are at most 500 bytes each.
- Alternatives not taken:
  - Refill on every closed row: one run per answer, and a plan that changes while the user reads it.
  - A host-side queue that merges plans: a second ranking next to the agent's, and rows the agent never saw.
  - Dimming closed rows at the end of the list: still noise, and it pushes the open rows apart from the header on a phone.

## Addendum (2026-11-11): rows say what to do

[ADR 0059](0059-today-rows-say-what-to-do.md) changes how §2 shows a row: the verb of its target comes before the title, and the preview leads with Do, Enter, Send and the plan keys. Plan Now is Re-rank in muxa for tmux, with the runs left today. The queue, Next and the refill stay as above.
