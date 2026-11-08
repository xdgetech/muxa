# 0044: A Task row opens its session; Task Info is in More

## Status

Proposed. The developer chose this on 2026-11-08. It changes ADR 0035 §2 for Tasks mode: a Task with a session no longer shows its detail first.

## Context

- In Tasks mode, a row shows the Task's detail: goal, status, Open (Chat, Terminal, TASK.md, Changes) and Restart. The work happens in the session, so every visit costs one more tap.
- A Sessions row opens its pane at once, in its last view. The developer wants a Task row to feel the same.
- The detail is still useful: the goal, the status, TASK.md, HANDOFF.md, Changes and Restart have no other place.

## Decision

1. **A Task row with a session opens that session** as a Sessions row does (ADR 0035 §3): its last view; the first time Chat when the agent has it, else Agent, else Shell. The row stays highlighted. On a phone the pane is pushed.
2. **A Task row without a session, or a closed Task, shows its detail** as before. There is no pane to show.
3. **The pane's More menu has Task Info** when the session runs a Task (`taskId`). It shows the Task's detail as a sheet. An action in it that opens a view (Chat, Terminal, a document, Changes) closes the sheet first.
4. Items, Routines and the switcher are unchanged.

## Consequences

- A Task is one tap from its session, like a manual session.
- The goal and status are one more tap away, under More.
- Alternatives not taken:
  - A Task Info button beside the view picker: the toolbar is full on a phone.
  - A tab between the detail and the pane: a second switch outside the pane (ADR 0035 rejected it for Chat).
