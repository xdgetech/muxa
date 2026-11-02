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
- **Data directories.** An adapter searches a list of directories, not one (ADR 0016): every `--agent-dir <id>=PATH`, else its default, then each workspace's value of the adapter's variable (`CLAUDE_CONFIG_DIR`, `CODEX_HOME`) from `[workspace.agent]`. The matching rules stay the same; the files of all directories compete under one order. The bridge reads the workspaces once at start for this.
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
2. Add it to `Registry::standard` and `data_dirs`, with a default data directory and the variable that moves it. `--agent-dir <id>=PATH` replaces the default and may repeat.
3. Add the sample files and unit tests, and an integration test like `tests/chat.rs`.

**Planned, not built:** an external adapter, an executable named on the command line with `--agent-exec NAME=PATH`, that receives the pane info as JSON and prints entries. It would plug in as one more `ChatSource`. The bridge will never search `PATH` for adapters.

## Task launchers

`new-task` starts a task through a **task launcher** (`crates/muxa-bridge/src/tasks/`, ADR 0007):

```rust
trait TaskLauncher: Send + Sync {
    fn id(&self) -> &'static str;   // for logs, e.g. "tmux-agents"
    fn launch<'a>(&'a self, client: &'a Client, req: &'a TaskRequest) -> Launch<'a>; // Launched {id, name, dir}
}
```

- `launch` returns the session's id, its name and the worktree. A launcher finds an existing session by its `@agent_worktree`, never by name (invariant 3).
- `tmux_agents.rs` runs tmux-agents' `new.sh` with `--no-switch`. It is picked when tmux-agents sets `@agents_scripts`, or with `--task-launcher tmux-agents=PATH`. It then finds the branch's worktree with `git worktree list`, and the session whose `@agent_worktree` is that path. `new.sh`'s printed name is used only in error messages. For a task record it also passes `--muxa-task <id>` when tmux-agents lists `muxa-task` in `@agents_features`, so the session has `@muxa_task` from its creation (ADR 0017).
- The worktree and naming rules belong to tmux-agents, and `builtin.rs` copies them. `crates/muxa-bridge/tests/tmux_agents.rs` runs both on the same branches and fails when they differ: `MUXA_TMUX_AGENTS=<tmux-agents checkout> make test-tmux`.
- `builtin.rs` makes the worktree with git and a detached session with the agent command (`--task-command`, default `claude`). It is picked otherwise, or with `--task-launcher builtin`.
- `close` closes a task's session and removes its worktree as the mode allows, never one with uncommitted changes. `tmux_agents.rs` runs `close.sh`; `builtin.rs` checks the same things itself.
- A workspace's `[workspace.agent]` (ADR 0016) sets the agent command and environment. The built-in launcher passes the environment as `new-session -e`, on the control client's stdin; only keys are logged. tmux-agents cannot pass it yet, so a Task whose workspace has that table always uses the built-in launcher. A `new-task` without a workspace takes the table of the one workspace whose `repos` lists the repo.
- `Builtin::launch_in_dir` is the directory mode for a task record without a repo (ADR 0015). It makes `<workspace root>/tasks/<slug>-<id>/` and a detached session of that name running `--task-command`, with the pane tagged `@muxa_role agent`. It is used even when tmux-agents is installed: `new.sh` needs a repo.
- `create-task` (`crates/muxa-bridge/src/task_records/`) runs a launcher, then tags the session `@muxa_task <id>` (again, when the launcher already did) and writes `.muxa/TASK.md`. In a repo, `.muxa/` goes into the exclude file of the repo's common git directory first. It then waits up to 30 s for the agent pane's state to be `idle`, and types `Read .muxa/TASK.md and start.` and Enter. It does not type on `waiting`: that state includes a permission menu, where the text would answer it. An agent whose adapter reports no state before its first turn does not get the prompt; the record's note then says to type it.
- `create-task` also adds the record's first run (ADR 0018). The prompt path fills in its `agent`, `agentSession` and `transcript` from `AgentAdapter::find`: `Found::session` is the agent's own session (Claude Code's `sessionId` and its `projects/` transcript; Codex's rollout id from `session_meta` and the rollout path). Paths only, never content.
- `TASK.md` has a marker line, `<!-- muxa:notes ...`. The bridge owns the header above it; the agent keeps Progress, Decisions and Next actions below it, and a rewrite keeps that part byte for byte (`task_records/brief.rs`).
- `restart-agent` (`task_records/restart.rs`) finds the Task's agent pane, refuses while it is `running` unless `force`, and rewrites `TASK.md`'s header. It writes `.muxa/HANDOFF.md` from facts (`task_records/facts.rs`): runs, the record's status and note, and git's branch, commits since the base and `status --short` for a repo, or changed files by mtime without one. Then `respawn-pane -k -t %n -c <dir> -e ...` restarts the same pane, with the workspace's env. A pane with no start command (tmux-agents types the agent into a shell) gets the agent command typed: the workspace's `command`, else `--task-command`. The prompt `Read .muxa/TASK.md and .muxa/HANDOFF.md, then continue.` waits for an `idle` whose `since` is at or after the restart, because a state tag survives the respawn.
- A launcher must never switch an attached client or change an existing pane when it starts a task. Closing moves the clients on the closed session to another one, as tmux-agents does. The request is checked before any launcher runs (`tasks::validate`).
- A client may leave right after its `create-task` result. The bridge still finishes the launch and the wait for the prompt (`task_records::Pending`, at most 100 s) before it exits.

## Desktop tools

A tool on the host, such as tmux-agents (ADR 0017), reads and creates Tasks through `muxa-cli` with `--json`. It never reads or writes the record files itself.

```sh
muxa-cli --bridge 'muxa-bridge [--socket S]' tasks --json
muxa-cli --bridge 'muxa-bridge [--socket S]' task new --workspace W --title T --goal G \
  [--repo R --branch B [--base REF]] [--project P] --json
```

- Each prints one line of JSON on stdout. A failure prints `muxa-cli: <code>: <message>` on stderr and exits non-zero.
- `tasks --json` prints `{"workspaces": [names], "truncated": bool, "tasks": [item, ...]}`.
- `task new --json` prints one item as soon as the record is saved. It then stays until the bridge has typed the start prompt, or gave up after 30 s. Read the first line; do not wait for the exit.
- `task restart ID [--force] --json` (ADR 0018) prints the restarted Task's item the same way.
- An item has:
  - `group`: `needs-attention`, `ready-for-review`, `running`, `no-agent` or `closed`. The rule is ADR 0015's, the same as in the apps (`crates/muxa-cli/src/inbox.rs`). Treat an unknown group as one without a known meaning.
  - `record`: the task record as `protocol/PROTOCOL.md` describes it.
  - `session`: optional, `{id, name}` of the session whose `taskId` is the record's id. Target it by `id`.
  - `agent`: optional, `{state, since?}` of that session's agent pane, only when it can be believed: not with a shell in the foreground, and not for a closed Task.
- Items come in group order (the order above), newest `updatedAt` first within a group.
- New fields can appear. Changing a name above is a breaking change.

## Claude Code

- `CLAUDE_CONFIG_DIR` (default `~/.claude`) moves everything below, so the adapter searches each workspace's too.
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

- `CODEX_HOME` (default `~/.codex`) moves `sessions/`; the adapter searches each workspace's too, and the newest matching rollout wins.
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
