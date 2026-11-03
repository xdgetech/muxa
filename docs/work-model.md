# Inbox, Tasks and Sessions

This defines the product concepts used by the vision and milestone plan. Proposed navigation changes are in ADR 0025; Routines are in ADR 0026. The concepts below do not claim that every lifecycle operation already has a UI or wire endpoint.

## Different kinds of things

| Concept | Question it answers | Identity and lifetime |
|---|---|---|
| Task | What result are we trying to achieve? | Durable work id, goal, workspace/project, status, requested worker and run history. It survives a closed terminal or agent process |
| tmux Session | Where can I run commands and interact with the work? | An actual tmux session containing windows/panes and their processes. It may be manual or explicitly linked to managed work |
| Agent run | Which agent execution did this work? | One recorded execution with observed native agent/session identity when available. Its owner is a Task or a Routine (ADR 0026). Restart creates a new run while the tmux session/pane can stay the same |
| Worker | Which reusable native role/profile should execute work? | Host configuration, not a Task, terminal or running process |
| Routine | What runs on a trigger, and what does it propose? | Host configuration with a stable id: workspace, project, trigger, worker, instructions and output policy. Its runs are not Tasks; they appear only in its history and as running work |
| Inbox entry | What needs my action now? | An ordered request for attention: a Task need, briefing or proposed action. Completing/deferring an Inbox action does not implicitly end its underlying Task/session |

A Session is not inherently manual, and a Task is not a renamed tmux Session. A manual shell or manually launched Claude/Codex can exist without a Task. A Task can have no live session, for example after its session ends. A person can type inside a Task's session while its Task identity remains unchanged. No process-name or terminal-content heuristic promotes a manual session into a Task.

## Example

1. Task `t-000001`: fix a login bug. The goal, notes and history belong to this Task.
2. tmux session `$7`, agent pane `%12`: the place where the work executes. The user can open it directly in Terminal.
3. Agent run 1 ends; restart creates run 2. The Task, tmux session and pane can all keep their identities. Native agent-session identity may change.
4. A review request enters Inbox and references this same Task. Opening it, moving it down or deferring it does not create another Task or mark the work Done.
5. Closing the tmux session does not prove the Task is complete. The durable Task record remains; completion is a separate host-confirmed action.

Conceptually a Task can continue in a replacement session. Current restart/recovery implements replacement in the existing pane. Starting a new session for an existing no-session Task, or adopting an existing manual session as a Task, still needs an explicit design/host operation; neither is already provided by this model.

A decision in Inbox has one of two results:

1. A Task need (approval, review): the user's answer continues the same Task. No new Task is created.
2. A Routine proposal: approval creates one new Task, which performs the approved action with its content unchanged. Approving twice still creates one Task. Dismissal never closes another Task.

A Routine run's execution belongs to the Routine's run history. It is not duplicated as a Task or a human ToDo.

## Destinations in the UI

- Inbox: ordered items needing the user's attention, with Act, Done, Move down and Later as appropriate.
- Tasks / All Tasks: managed work, including running work, goals, progress, worker selection and history. A Task needing input or review also has an Inbox reference; it remains the same Task.
- Manual sessions: direct tmux browsing and interaction, defaulting to sessions unassociated with a Task/Routine. The label may be Sessions, Manual or Terminal; final naming/layout is not frozen. An All Sessions filter exposes managed sessions too. Task/Routine detail keeps Open Terminal.

These are navigation destinations, not mutually exclusive copies of domain objects. The manual-session filter is why earlier UI notes describe “Sessions = manual work”; the underlying Session concept includes managed execution as well. Actor changes (human versus AI typing) never change classification.

## Milestone implications

Q0 freezes the distinct identities, Task-to-Inbox references and explicit session associations. Q1 separates the destinations while preserving access to existing manual tmux work. Q2 adds shared order/deferral and proposal acceptance without coupling Inbox completion to Task completion. W2/W3 preserve the distinction between worker assignment, agent run and tmux Session. S0–S3 (ADR 0026) add Routines; a Routine run is stored as a record of kind `routine-run`, which the Tasks destination skips. M remains independent of queue/scheduler delivery.
