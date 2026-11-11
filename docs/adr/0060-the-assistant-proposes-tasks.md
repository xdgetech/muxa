# 0060: The assistant proposes Tasks

## Status

Accepted on 2026-11-11. It amends ADR 0048 §4 (the result file gains `task`), ADR 0051 §3 (a chain's proposal is the lead's), ADR 0058 §2 (a `result` item's Start Task) and ADR 0054 (the secretary proposes instead of asking yes in chat). Hosts advertise `work:18`. The host, the protocol, `muxa-cli`, muxa for tmux, the Android client and the iOS client are built.

## Context

The user: "I asked my assistant to fix a bug in muxa. The staff replied in the Inbox. I asked it to create a Task, and it says it is a researcher, so it cannot create a Task. Ideally the assistant knows it needs a Task and where, it is created, and I get an Inbox item to review the plan and approve it or not before the implementation starts. I also want to talk to the Task directly to give more context."

- A staff run's scope says "change nothing outside this folder" (ADR 0048 §4). Its Result can propose a hand-off (ADR 0051) or a write (ADR 0052), not a Task. So a request that needs code ends in "I cannot".
- `create-task` with `work` starts a Task linked to a Work item (ADR 0048). Only the user starts one, from the New Task form or the Work menu (ADR 0059 addendum).
- The secretary's brief (ADR 0054) asks for a yes in the chat before `muxa-cli task new`. The yes stays in the conversation, not in the Inbox.
- Claude Code has a plan mode. Its plan menu becomes an `ask` item of prompt kind `plan`, but the Inbox can only keep planning: which mode `1` approves with depends on settings (`docs/agents.md`).

## Decision

### 1. A Result may propose one Task

`result.json` gains `task {workspace?, repo?, title, goal}`, at most one per Result.

- `workspace` may only be the request's own. A Task elsewhere crosses companies, which only the user asks for (ADR 0051 §4); the run writes it in `openQuestions`.
- `repo` names one of the workspace's repos: under its root, or listed. Without it, the Task gets its own folder under `<root>/tasks/`, as `create-task` without `repo` does.
- `title` is one line of at most 200 characters, `goal` 1 to 4096 bytes. Reasons never repeat `repo`.
- The brief's result section says: when the request needs code or other changes on this host, propose a Task; never answer that you cannot. It lists the workspace's repos.

**Which proposal a `result` item offers.** Of the version's steps up to the item's, the newest proposal of a step the lead ran; else the newest proposal of any step. So the lead's proposal wins over a member's, a member's proposal still shows when the lead made none, and under `handoffs = "auto"` the last item carries an earlier step's proposal. `result.task` names the `step` and `worker` that proposed it.

### 2. Start Task on a `result` item

| The item offers | `answers` | Start Task is |
|---|---|---|
| a Task, no hand-off | `approve`, `reply`, `dismiss`, `later` | `approve` |
| a Task and a hand-off | `approve`, `start-task`, `reply`, `dismiss`, `later` | `start-task`; `approve` still hands off |

Start Task makes one Task through `create-task`'s path: the proposal's workspace, repo and title, the workspace's default Task worker, `work` = the request's Work item, in plan mode (§4). The goal is the proposal's goal, then who proposed it and "Plan first", then the last line `muxa-task-proposal: <request> v<version> s<step>`. That marker is per proposal, not per item, so a second answer, or Start Task on a later item that offers the same proposal, returns the same Task. The approval is kept on the item as a write's is (ADR 0052 §3), and a crash between the Task and its record finds the Task by the marker.

The item is answered with the action given, `task` is the new Task, and its summary starts "Started Task `<id>`: `<title>`". On an item that also offered a hand-off, the chain ends there, as Done ends it. Send (`reply`) keeps ADR 0058 §3: the member writes a new version, whose Result can propose a changed Task.

`start-task` is a new action so that a client before level 18 never sends it by mistake; such a client sees `approve` only, which hands off when there is a hand-off and starts the Task when there is none. Not taken: `approve` meaning both on one item; a client could not tell which it pressed.

### 3. The secretary proposes with `muxa-cli task propose`

`propose-task {workspace, title, goal, repo?, work?}` (CLI: `muxa-cli task propose --workspace W [--repo R] --title T --goal - [--work WORK]`) makes one item of kind `new-task` with `newTask {workspace, repo?, title, goal, work?}`. Its answers are `approve` (Start Task, as §2, marker `muxa-task-proposal: <item> <hash>`), `reply`, `dismiss` (Done) and `later`.

- `reply` (Send) types the proposal and the user's text into the secretary's session, the overall workspace's root Ask session, with ADR 0059's rules for a menu or a person typing. The item stays open. The secretary proposes again with the change.
- The same proposal while its item is open returns that item.
- A new kind, not `proposal`: a `proposal` is a Routine's action with a target and content, approved under ADR 0026's hash and `max_writes`. A `new-task` item is a Task to start, with no Routine and no write.
- The items live in the item file `t-0`, which belongs to no run; record ids have six or more hex digits.

`docs/examples/secretary-CLAUDE.md`: for work that needs code, propose a Task with this command instead of asking yes in the chat.

### 4. Tasks from a proposal start in plan mode

`NewTask` gains `plan`. Only Start Task sets it; a Task the user starts keeps today's start.

- Claude Code gets `--permission-mode plan`: added to a worker's argv, or to a shell command line whose program is `claude` (the rule of `reading::is_claude_code`).
- tmux-agents' `new.sh` takes `--plan` and advertises `plan-mode` in `@agents_features`; `layout.sh --plan` adds the flag when the launch command's program is `claude`. An older `new.sh` gets no flag: the agent starts as usual, and the host logs that.
- **Codex** has no plan flag that muxa uses. It starts as usual. The goal's "Plan first" asks it to present a plan and wait; Codex reports no menu (`docs/agents.md`), so its plan arrives as a `review` item, and Send ("go ahead") answers it.
- A restart (`restart-agent`) does not plan again.

### 5. Approve the plan from the Inbox

`docs/findings/claude-code-plan-menu.md` records the plan menu, checked without a model turn (`scripts/repro/claude-plan-menu.sh`). The option that leaves plan mode and asks before each edit is "Yes, manually approve edits"; its number depends on settings, and a digit picks it without Enter.

- A `plan` item with a `pane` lists `approve`, `reject` and `reply`.
- `approve` (Approve plan) presses that option only while the adapter reports the same plan menu and the screen shows exactly one line `<n>. Yes, manually approve edits`. Else `conflict`: "approve the plan in the Task's terminal". It never presses an auto-accept or bypass option.
- `reject` (Keep planning) presses `Escape`. `reply` (Keep planning & Send) presses `Escape`, waits for the turn to end, and sends the text, as on a permission: the probe showed the turn ends and the next text is a prompt.
- The chat's `prompt` keeps only Keep planning; a Chat client approves in the terminal.

### 6. Talking to the Task

- An item that started a Task (a `result`, `new-task` or `write` item) has `task`. muxa for tmux shows `o Open its Task` there, and Enter or `o` opens the Task's session.
- Task items (`review`, `ask`, the plan item) keep `s`: Send to the Task's agent (ADR 0059 §4).
- The Work menu (ADR 0059 addendum) lists, for each live Task, `Open its Task` and, when the Task has an open item that takes `reply`, `Send to its Task's agent`.

### 7. Wire: additive, `work:18`, protocol v1

`TaskProposal {workspace, repo?, title, goal, work?, step?, worker?}`; `AssistResult.task`; `ResultReview.task`; `InboxItem.newTask`; `ItemKind` `new-task`; `InboxAction` `start-task`; request `propose-task`. Fixtures: `protocol/fixtures/task-proposals/`, a new folder so that the iOS and Android fixture tests stay green until they read it.

### 8. Clients: a follow-up

1. Types: `TaskProposal`, `ResultReview.task`, `AssistResult.task`, `InboxItem.newTask`, `ItemKind.newTask` (`new-task`), `InboxAction.startTask` (`start-task`), `Request.proposeTask`, `workTaskProposals = 18`, in Swift and Kotlin. Their fixture tests read `protocol/fixtures/task-proposals/`.
2. A `result` item with `result.task`: a Proposed Task card (title, workspace, repo, goal, "Starts in plan mode"); the main button Start Task (`approve` without a hand-off, `start-task` beside one).
3. A `new-task` item: the same card, Start Task, Send ("Tell the secretary what to change…"), Done.
4. An item with `task` after Start Task: Open its Task.
5. A `plan` item: Approve plan (edits one by one), Keep planning, and a reply box "Keep planning & send…"; a `conflict` says to approve in the terminal and offers Open Session.
6. Today: the verb of a `new-task` row is Start Task.

### 9. Logs

Logs carry item, Task and request ids, the answer and whether plan mode was asked. Never a title, a goal, a repo name, the text or the screen. A Task's session is named from its title, so the plan-mode line carries no session name.

## Consequences

- A request that needs code ends in a Task to start, not a refusal; the user decides in the Inbox, then reviews the plan before any edit.
- One more kind and one more action on the wire. Old clients keep working: `new-task` shows as unknown with working buttons, and they never send `start-task`.
- The plan approval depends on Claude Code's menu text. After an upgrade the probe says whether it still holds; until then a changed menu makes approve a `conflict`, never a wrong key.
- Alternatives not taken:
  - Letting a staff member create the Task itself: a researcher's permissions stay read-only (ADR 0051 §1), and the user would see the Task only after it started.
  - Reusing a `write` item for the Task: a write is text for another system, approved by hash and counted against `max_writes_per_day`.
  - Pressing `1` on the plan menu: in the probe's setup `1` is "Yes, and use auto mode".
  - Sending Shift+Tab to enter plan mode after start: a key guess against a changing cycle.
