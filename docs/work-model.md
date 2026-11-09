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
| Inbox item | What must I decide now? | One stored question (ADR 0033): a Task's agent asks, waits for review or failed, or a Routine proposes or briefs. It has its own read and answered state. Answering may change its Task (Accept, Drop) but the item is not the Task |

## Unstarted work and the assistant (ADR 0046)

| Concept | Question it answers | Identity and lifetime |
|---|---|---|
| Capture | What did I write down? | `c-<8hex>`: the original text (at most 16 KiB, never rewritten), its source and an optional workspace. `received` (Unsorted) until the user sorts it into a Work item or keeps it as a note. Never pruned automatically |
| Work item | What do I intend to do, before it is a Task? | `w-<8hex>`: title, workspace, `open` / `done` / `withdrawn`, Later. It links Captures, assistant requests and Tasks. `in-progress` and `waiting-decision` are derived from its Tasks and requests |
| Assistant request | What did I ask a workspace's agent to find out? | `q-<8hex>`: one Work item, a scope (`research`, `compare`, `draft`), a purpose, versions, a derived delivery state, and up to 10 Results. A newer version makes older Results `stale` |
| Assistant run | Which execution answers a request version? | A record of kind `assistant-run`, like a `routine-run`: hidden from Tasks and Inbox items, with its own folder and session. One per request version |

- A Capture and a Work item link many-to-many. Nothing links, merges or discards them without the user.
- A Work item is not a Task. Starting a Task from it (`create-task` with `work`) makes one Task per Work item `rev`, and the Task keeps `work`. The item stays open until the user completes it.
- A Result never changes the Work item's state, and never starts a Task. The user reads it and decides.
- An assistant run uses the Routine machinery (create path, delivery, Loop, in-process check), but it belongs to a request, not to a Routine. A Routine is configured and triggered; a request is asked once per version by the user.
- A decision a Task needs stays an Inbox item. Its Work item shows "waiting on decision" and has no answer of its own.

## Plans and the overall assistant (ADR 0047)

| Concept | Question it answers | Identity and lifetime |
|---|---|---|
| Delegate notes | What does a workspace's assistant remember between runs? | `<root>/.muxa/assistant/NOTES.md`, one per workspace with `[workspace.assistant]`. A run reads a copy and may propose a new text; it is written only when the user applies it, or by the host with `notes = "auto"` |
| Overall assistant | Who looks across workspaces? | One workspace with `overall = true`. It reads only what each workspace's `share` allows, and asks other workspaces only through proposals |
| Plan | What should I do next? | `p-<8hex>`: one overall run's answer, at most 3 rows with reasons and minutes. Made on a schedule or by Plan Now. The host keeps 7 |
| Proposal | What does the overall assistant want to do for me? | An Inbox item of kind `sort` or `delegate`, from the newest plan only |

- A Work item's `pinnedAt` and `due` are the user's. A pinned item is listed first and fills the first plan rows. A plan's estimated deadline stays in its `basis`, never on the item.
- A plan never changes a Work item, an Inbox item, their order or `laterAt`. An item moved down with Later is never a row; when it is due soon, it shows in one line under the rows.
- A `sort` proposal, approved, does what `sort-capture` does to the Capture. A `delegate` proposal, approved, makes an assistant request on the Work item, as `request-assistant` does. Approving twice makes one request. Rejecting changes nothing else.
- The overall assistant never sends a request without approval, unless the receiving workspace sets `overall_requests = "auto"`. Even then the Inbox item stays, answered, so every request it sent is visible.

A Session is not inherently manual, and a Task is not a renamed tmux Session. A manual shell or manually launched Claude/Codex can exist without a Task. A Task can have no live session, for example after its session ends. A person can type inside a Task's session while its Task identity remains unchanged. No process-name or terminal-content heuristic promotes a manual session into a Task.

## Example

1. Task `t-000001`: fix a login bug. The goal, notes and history belong to this Task.
2. tmux session `$7`, agent pane `%12`: the place where the work executes. The user can open it directly in Terminal.
3. Agent run 1 ends; restart creates run 2. The Task, tmux session and pane can all keep their identities. Native agent-session identity may change.
4. A review item enters Inbox and references this same Task. Opening it or deferring it does not create another Task or mark the work Done. Accept does mark it Done.
5. Closing the tmux session does not prove the Task is complete. The durable Task record remains; completion is a separate host-confirmed action.

Conceptually a Task can continue in a replacement session. Current restart/recovery implements replacement in the existing pane. Starting a new session for an existing no-session Task, or adopting an existing manual session as a Task, still needs an explicit design/host operation; neither is already provided by this model.

A decision in Inbox has one of two results:

1. A Task need (approval, review): the user's answer continues the same Task. No new Task is created.
2. A Routine proposal: approval creates one new Task, which performs the approved action with its content unchanged. Approving twice still creates one Task. Dismissal never closes another Task.

A Routine run's execution belongs to the Routine's run history. It is not duplicated as a Task or a human ToDo.

## Destinations in the UI

- Inbox: the questions waiting on the user, each with its evidence and answers, and Later (ADR 0033).
- Tasks / All Tasks: managed work, including running work, goals, progress, worker selection and history. A Task needing input or review also has an Inbox item; it remains the same Task.
- Sessions: direct tmux browsing and interaction. It lists manual sessions only: sessions with no Task association. It has no All Sessions filter (ADR 0027). Task sessions open from Inbox, All Tasks and a Task's Open Terminal. The session switcher (Cmd+K, Ctrl+Shift+K) searches every session, Task sessions and Task titles too.

These are navigation destinations, not mutually exclusive copies of domain objects. Sessions shows manual sessions only, which is why UI notes describe “Sessions = manual work”; the underlying Session concept includes managed execution as well. Only the Task association classifies a session. Actor changes (human versus AI typing) never change classification.

## Milestone implications

Q0 freezes the distinct identities, Task-to-Inbox references and explicit session associations. Q1 separates the destinations while preserving access to existing manual tmux work. Q2 adds shared order/deferral and proposal acceptance without coupling Inbox completion to Task completion. W2/W3 preserve the distinction between worker assignment, agent run and tmux Session. S0–S3 (ADR 0026) add Routines; a Routine run is stored as a record of kind `routine-run`, which the Tasks destination skips. M remains independent of queue/scheduler delivery.
