# muxa vision

This is where muxa is heading. Only the current slice in `docs/plan.md` (Next) is a commitment. Everything else here is direction, not a promise, and can change.

## In one sentence

muxa grows from a remote view of your tmux panes into a workspace where work runs with AI agents: coding, and also product, strategy, finance and admin work.

## How it grows

| Phase | What muxa adds | State |
|---|---|---|
| 1. Remote tmux workspace | Your existing panes on a phone or tablet, at their real size, without changing the desktop layout. | Built |
| 1b. Agent-aware workspace | Agent states, chat, approval cards, alerts, the Reader, starting and closing worktree sessions. | Built |
| 2. Workspace and Task foundation | Durable Tasks with a goal, grouped by Workspace; an Inbox of what needs you. | Built first slice: records, workspaces, Inbox and non-repo Tasks; real-account/device verification and ADR acceptance remain |
| 3. Task execution | muxa launches and follows agent runs for a Task, and keeps their history. | Partly built: launch, run history, handoff and daemon loops; default-agent restart recovery implemented, Workers in progress, workspace memory not implemented |
| 4. Workflow orchestration | Tasks connected in a graph: one step's result starts the next. | Direction |
| 5. Autonomous work | Agents pick up and finish Tasks with little supervision, inside the limits you set. | Direction |

Each phase keeps everything before it. “Built” describes local code and validation, not a published product or completed real-device/native-agent release verification.

The current execution work is split into Workers/handoff (W) and workspace memory/close-out (M), tracked in [the delivery plan](workers.md). Worker configuration, internal native-argv create/restart integration and capability-gated CLI/iOS/Android selection are prepared. Managed restart state, input and Chat now require fresh process-bound observation; authenticated native role/profile discovery, named startup/delivery and release gates remain. Default-agent restart recovery is implemented independently of named Workers. Memory has an adapter/source audit and acceptance matrix, but no complete snapshot/proposal/apply implementation. In particular, the existing normalized agent state cannot distinguish some same-second completed turns, so close-out readiness cannot safely reuse it unchanged. Workspace environment selection is implemented, but real-account checks remain; workspace scope lists are not an OS security boundary. Workflow orchestration and autonomous execution have no implementation commitment yet.

## Inbox and recurring project intake

Manual tmux work has a separate Sessions destination, while managed execution/history lives under Tasks. Association determines the category: manually launched AI stays manual, and human typing inside a Task keeps its Task identity.

The desired daily surface is an ordered Inbox list: work from the top, complete an action, move it down, or defer it until a chosen time. Briefings and proposed work can enter Inbox before an agent Task exists. Proposed ADR 0025 extends ADR 0023 with host-owned order and cross-device dispositions; today's grouped List/Triage and local Snooze remain the implemented slice.

Routines bridge Phases 3 and 4. A Routine runs a worker on a trigger, for example every morning, while clients are disconnected. The worker reads its sources (Slack, Linear, Notion and others) through its own native tools, works in the Task Loop engine, and publishes briefings and proposals to Inbox. An approved proposal becomes a new Task that performs the action. Routine runs are not Tasks. Routines do not require the full workflow graph or autonomous execution. See [ADR 0026](adr/0026-routines.md).

For the shared domain and UI definitions, see [Inbox, Tasks and Sessions](work-model.md); the integrated W/Q/S/M milestones are in [the delivery plan](workers.md#product-milestones-across-the-vision).

## Concepts

These stay separate. Merging two of them makes a later phase hard.

| Concept | What it is |
|---|---|
| Workspace | A body of work with its own root directory and its own credentials, for example a company or a personal area. |
| Project | A goal inside a Workspace that groups Tasks. For now a free-text field on a Task. |
| Task | Durable managed work: a goal, status and history that outlive its execution session. |
| Resource | What a Task works on: a repo, documents, an issue tracker. For now zero or one repo. |
| tmux session | The actual execution/interaction location, containing windows/panes; can be manual or associated with managed work. |
| Agent run | One agent execution on a Task. Restart can create a new run in the same tmux session/pane; state comes from an adapter. |
| Inbox entry | A request for human action, referencing a Task need, briefing or proposed work; not another copy of the Task. |
| Routine | A worker run on a trigger that publishes briefings and proposals to Inbox; an approved proposal becomes a new Task. Proposed in ADR 0026. |
| Workflow | A graph of Tasks and the rules that connect them. |

A Task is not a tmux session. A session can end and the Task continues. A Task can have several agent runs over time.

## Three modes

1. **Manual.** You work in your panes yourself, from the desktop or from muxa. This is muxa today.
2. **Task.** You give a goal. muxa starts an agent on it, shows its state, and you review the result.
3. **Autonomous.** muxa starts and connects Tasks on its own, within the gates you set.

Manual Mode always stays. Every Task runs in a tmux session you can open and type into. No mode takes the terminal away.

## What muxa does and does not do

- muxa does **not** reimplement the agent loop. Claude Code, Codex and other agents do the thinking and the tool calls. muxa starts them, reads their state through adapters, and shows their output.
- muxa **owns** the Task loop and the workflow graph: which Task runs, when it counts as done, and what comes next.
- muxa keeps **persistent memory** for a Task. The first form is `TASK.md`: the goal and context, written where the agent can read it. A new agent session reads the same file. This is the start of context handoff between runs.
- **Approval gates** stop important actions until a human agrees: pushing, merging, spending money, sending messages outside the workspace. Today's approval cards (ADR 0009) are the first gate.
- **Workspace isolation.** Each Workspace has its own credentials and directories. An agent in one Workspace does not get another's.
- **OSS first.** The bridge, the protocol and the clients stay open source. A client of your own can rely on `protocol/PROTOCOL.md`.

## Principles that do not change

- The window size follows the device in use, and nothing else changes the layout (AGENTS.md, invariant 2; ADR 0020).
- No invented agent state. Report what an adapter supplies, or `unknown`.
- Task content, like pane bytes and transcripts, never reaches logs.
- The host decides what an agent may access. A client asks; the host's config allows or refuses.

## Open questions

- The hierarchy above Task: whether Project becomes an entity, and how Resources attach to it.
- When the Task loop needs a process that runs while no client is connected (a daemon). ADR 0015 names the triggers.
- How a Workspace stores its credentials, and how an agent receives them.
