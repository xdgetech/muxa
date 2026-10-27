# Agent adapters

The chat view ("Pretty") shows a coding agent's conversation as messages instead of a terminal. The bridge builds it from what the agent itself records. An **agent adapter** knows one agent: where it keeps its records and how to read them. The rest of muxa, including the protocol and the apps, never sees an agent's own format.

Adapters live in `crates/muxa-bridge/src/agents/`: Claude Code (`claude.rs`) and Codex (`codex.rs`). `log.rs` holds the bounded entry list they share.

## Contract

```rust
trait AgentAdapter: Send + Sync {
    fn id(&self) -> &'static str;   // shown to clients, e.g. "claude-code"
    fn open(&self, pane: &PaneInfo, procs: &Processes) -> Result<Option<Box<dyn ChatSource>>, Error>;
}

trait ChatSource: Send {
    fn poll(&mut self, pane: &PaneInfo) -> Result<Vec<ChatEvent>, Error>; // first call: a Reset
    fn entries(&self) -> Vec<ChatEntry>;  // everything current, for a resend
    fn status(&self) -> Option<Agent>;    // None when unknown
    fn prompt(&self) -> Option<ChatPrompt>; // optional: what the agent waits on, with its keys
}
```

- **`open`** gets the pane (`id`, session name, `pane_pid`, path, whether a shell is in front) and a snapshot of the process tree. It returns `None` when its agent is not running in that pane. The registry (`Registry::standard`) asks the adapters in order, and the first source wins.
- **`poll`** runs every 500 ms on the blocking pool. It returns `Reset` (replace everything) or `Upsert` (add an entry, or replace the one with the same id). A source keeps whatever state it needs, for example to pair a tool call with its later result. `Error::Gone` ends the chat, for example when the agent exited.
- **`find`** (optional, None by default) says whether the adapter's agent runs in a pane, and its state, without opening it. The bridge uses it to find the agent pane of a window that no tag describes, and the state of an agent pane that has no state tag (ADR 0008). Claude Code uses the same session-file lookup as `open`, and its `status`.
- **`prompt`** (optional, None by default) names what the agent waits on and the keys that pick each choice, for the approval card (ADR 0009). Return one only when the agent's records show exactly what it waits on, and only with keys checked against that agent's menu.
- **`documents`** (optional, empty by default) returns files the agent keeps outside the repo, such as its plan, for the Reader. Each comes with the directory it must stay in.
- **The chat task** (`src/chat.rs`) is agent-neutral. It owns the protocol side: sequence numbers, batching, resending after the client fell behind, and checking the pane every 2 s.

## Rules for an adapter

1. **Find the agent by facts, not guesses.** Match it to the pane by ids the agent records, and check that its process still runs inside the pane (`Processes::is_within`). When the agent records no pane, as Codex does, match its process by name and its files by cwd and time, and document the limits. Pane ids restart with the tmux server, so match the session name too. If nothing matches, return `None`; the client then shows the terminal.
2. **Skip what you don't understand.** Agent formats are private and change. An unknown line or field is dropped, never turned into a made-up entry.
3. **Report state only when the agent records it** (AGENTS.md, invariant 6). When the pane is back at a shell (`shell_foreground`), report `None`.
4. **Never log content.** Log ids, sizes and errors. Transcripts get the same rule as pane bytes (invariant 5).
5. **Bound everything.** Use `tail::JsonlTail` for append-only JSONL files: it starts from the last 1 MiB, skips lines over 8 MiB, and reports a rewritten file. Cap entry text and tool results.
6. **Values that become paths are validated**, for example a session id.
7. **Pin the format.** Check in a sanitized sample of the agent's files under `crates/muxa-bridge/tests/data/<agent>/`, with the agent version it came from, and test against it.

## Adding an adapter

1. Add `src/agents/<agent>.rs` with the adapter and its source.
2. Add it to `Registry::standard`, with a default data directory. `--agent-dir <id>=PATH` overrides it.
3. Add the sample files and unit tests, and an integration test like `tests/chat.rs`.

**Planned, not built:** an external adapter, an executable named on the command line with `--agent-exec NAME=PATH`, that receives the pane info as JSON and prints entries. It would plug in as one more `ChatSource`. The bridge will never search `PATH` for adapters.

## Task launchers

`new-task` starts a task through a **task launcher** (`crates/muxa-bridge/src/tasks/`, ADR 0007):

```rust
trait TaskLauncher: Send + Sync {
    fn id(&self) -> &'static str;   // for logs, e.g. "tmux-agents"
    fn launch<'a>(&'a self, client: &'a Client, req: &'a TaskRequest) -> Launch<'a>; // the session name
}
```

- `tmux_agents.rs` runs tmux-agents' `new.sh` with `--no-switch`. It is picked when tmux-agents sets `@agents_scripts`, or with `--task-launcher tmux-agents=PATH`.
- `builtin.rs` makes the worktree with git and a detached session with the agent command (`--task-command`, default `claude`). It is picked otherwise, or with `--task-launcher builtin`.
- `close` closes a task's session and removes its worktree as the mode allows, never one with uncommitted changes. `tmux_agents.rs` runs `close.sh`; `builtin.rs` checks the same things itself.
- A launcher must never switch an attached client or change an existing pane when it starts a task. Closing moves the clients on the closed session to another one, as tmux-agents does. The request is checked before any launcher runs (`tasks::validate`).

## Claude Code

- `~/.claude/sessions/<pid>.json`: one file per running instance, with `sessionId`, `cwd`, `kind`, `status` (`busy`, `waiting`, `idle`) and `tmux` (`<session>:@<window>.%<pane>`). Only `kind: interactive` instances own a pane. Files of exited instances can remain, which is why the pid must still run inside the pane.
- `~/.claude/projects/<cwd with non-alphanumerics as ->/<sessionId>.jsonl`: the conversation.
  - `user` lines hold prompts or `tool_result` blocks.
  - `assistant` lines hold one content block each (`text`, `tool_use`, `thinking`). Lines of one turn share `message.id` and are merged.
  - `isSidechain` (subagents) and `isMeta` lines are skipped, and so is thinking.
- `/clear` and `/resume` keep the process but change `sessionId`. The source then resets to the new transcript.
- After `/compact` or a rewind, the file is not linear. The chat may show messages that Claude Code no longer shows.
- Every transcript line names the session's plan as `slug`. Plan mode writes it to `~/.claude/plans/<slug>.md`, which `documents` returns.
- While it waits (`status: waiting`), the last transcript line is the `tool_use` it waits on, with its full input. This was checked with 2.1.310 for a permission, AskUserQuestion and ExitPlanMode. The menu itself is not in the transcript. `claude/prompt.rs` offers the keys that held in every menu seen (ADR 0009):
  - a permission: `1` allows, `Escape` denies and interrupts the turn;
  - one AskUserQuestion question with a single answer: the option's number picks it;
  - ExitPlanMode: `Escape` keeps planning. Which mode `1` approves with depends on settings, so approving stays in the terminal.
  
  With several open calls, several questions or several answers, no prompt is sent. Neither is one while a subagent (`Task`, `Agent`) runs: it asks about its own calls, which are sidechain lines.

## Codex

Checked with Codex CLI 0.160.0.

- Codex records no pane and no pid. The adapter finds a process named `codex` below `pane_pid` (`Processes::named_within`) and takes its start time from `ps`.
- `~/.codex/sessions/YYYY/MM/DD/rollout-<time>-<id>.jsonl`: one conversation. The first line, `session_meta`, holds its `cwd`. The adapter follows the newest rollout whose `cwd` is the pane's path and that was written since the process started. Day directories from the day before the start are searched.
- `response_item` lines hold the conversation:
  - `message` with role `user` or `assistant`. `developer` messages, the `# AGENTS.md` text and whole tagged blocks such as `<environment_context>` are Codex's own context and are skipped.
  - `function_call` and `custom_tool_call` (for example `exec`), paired with their `*_output` by `call_id`.
  - `reasoning` is skipped.
- `event_msg` lines `task_started` and `task_complete` give the state, Running or Idle, with `started_at` and `completed_at` as its time. `turn_aborted` is Idle.
- Codex does not record an approval prompt, so a Codex agent never shows `waiting`.
- Every 4 s the source checks that the process still runs, and switches to a newer rollout (`/new`).
- Limits:
  - Two Codex instances in one directory are not told apart. Both panes follow the newest rollout.
  - A resumed conversation from an older day directory is not found.
