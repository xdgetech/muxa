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
- `create-task` (`crates/muxa-bridge/src/task_records/`) runs a launcher, then tags the session `@muxa_task <id>` (again, when the launcher already did) and writes `.muxa/TASK.md`. In a repo, `.muxa/` goes into the exclude file of the repo's common git directory first. It then waits up to 30 s for a finished turn in the agent pane, and types `Read .muxa/TASK.md and start.` and Enter. A finished turn is `idle`, or `done` with a `since` at or after the wait began: tmux-agents keeps a finished turn `done` while no person watches the window. It does not type on `waiting`: that state includes a permission menu, where the text would answer it. An agent whose adapter reports no state before its first turn does not get the prompt; the record's note then says to type it. The wait is written as `pendingPrompt`, so a daemon can resume an unclaimed wait within 30 s. Delivery itself is durably claimed before typing; uncertain claimed delivery requires explicit recovery rather than automatic resend.
- `create-task` also adds the record's first run (ADR 0018). The prompt path fills in its `agent`, `agentSession` and `transcript` from `AgentAdapter::find`: `Found::session` is the agent's own session (Claude Code's `sessionId` and its `projects/` transcript; Codex's rollout id from `session_meta` and the rollout path). Paths only, never content.
- `TASK.md` has a marker line, `<!-- muxa:notes ...`. The bridge owns the header above it; the agent keeps Progress, Decisions and Next actions below it, and a rewrite keeps that part byte for byte (`task_records/brief.rs`).
- `restart-agent` (`task_records/restart.rs`) finds the Task's agent pane and refuses a running agent unless `force`. Before taking the shared Task claim it resolves one snapshot of the configured workspace command (else `--task-command`) and workspace env, stages the merged TASK.md and factual HANDOFF.md, persists dispatch intent, and explicitly respawns in the same pane. Both files must be regular UTF-8 without NUL and at most 64 KiB; invalid content fails before process replacement. The agent's notes remain intact. Files are published only after successful replacement identity is recorded. The new run keeps a private observation baseline: its replacement PID/start identity, tmux session/pane and earlier observed agent session/transcript. The fixed prompt `Read .muxa/TASK.md and .muxa/HANDOFF.md, then continue.` requires fresh process-bound adapter evidence; retained pane tags never authorize it. The sender rechecks the agent and PID/start identity while holding the delivery claim. Hook-only custom agents and Codex’s current cwd/time-based matching cannot establish this binding: after 30 s the Task note asks you to type the prompt in Terminal. New runs remain unknown until sufficient evidence exists; baseline-free legacy waits and manual sessions keep their existing behavior. Startup/restart/loop delivery is never automatically repeated after an uncertain send. See ADR 0024 for process identity, publication and crash limits.
- A launcher must never switch an attached client or change an existing pane when it starts a task. Closing moves the clients on the closed session to another one, as tmux-agents does. The request is checked before any launcher runs (`tasks::validate`).
- A client may leave right after its `create-task` result. When the daemon serves the request, the daemon finishes the launch and the wait. Otherwise the bridge does, before it exits (`task_records::Pending`, at most 100 s).

## The host daemon and the Task Loop

The daemon (`muxa-bridge daemon run`, `crates/muxa-bridge/src/daemon/`, ADR 0019) serves Task records and runs the Task Loop (`src/loops/`). It uses the same adapters, launchers and record code as the bridge. Install and stop it as the README says.

- **Record requests.** `list-tasks`, `create-task`, `update-task`, `restart-agent`, `start-loop` and `stop-loop` all go through `task_records::handle`. A bridge sends each one to the daemon on `daemon.sock`, one connection per request, when a daemon of its own version answers (`task_records/remote.rs`). Otherwise the bridge runs `handle` itself. `start-loop`, `stop-loop` and `create-task` with a `loop` have no fallback: they are `unsupported` without a usable daemon.
- **Several writers.** The daemon, a fallback bridge and `muxa-cli` can all write a record. `rev` and a lock file per record keep each write whole; a loser gets `conflict`. Every bridge looks at `tasks/` every 2 s and sends `tasks-changed` on a change.
- **What the loop needs from an adapter.** The agent state and its `since`, from tags or `AgentAdapter::find` for manual/legacy runs. A managed restarted run first passes the same process-bound observation filter used by the session list; stale tags and uncertain run metadata supply no state. A finished turn is `idle` or `done` with a `since` at or after the loop's `armedAt`. A state with no `since` is never a finished turn. `waiting` pauses the loop, and `error` stops it. The loop never reads the pane's screen, so an adapter that reports no state gives the loop nothing to act on.
- **Typing.** The loop types its fixed prompt with `send-keys -l` and Enter, only on a finished turn, and only when no desktop client on the Task's session was active in the last 5 s (`loops/typing.rs`). It never sends menu keys.
- **The adapters' directories** come from the daemon's own flags and `workspaces.toml`, read when it starts. Restart the daemon after a change to `[workspace.agent]`.

## Desktop tools

A tool on the host, such as tmux-agents (ADR 0017), reads and creates Tasks through `muxa-cli` with `--json`. It never reads or writes the record files itself.

```sh
muxa-cli --bridge 'muxa-bridge [--socket S]' tasks --json
muxa-cli --bridge 'muxa-bridge [--socket S]' task new --workspace W --title T --goal G \
  [--repo R --branch B [--base REF]] [--project P] --json
```

- Each prints one line of JSON on stdout. A failure prints `muxa-cli: <code>: <message>` on stderr and exits non-zero.
- `tasks --json` prints `{"workspaces": [names], "truncated": bool, "tasks": [item, ...], "queue": [ids]}`. `queue` is the Inbox queue now: the ids of the Tasks that need the user, with Move down and Later applied (`protocol/PROTOCOL.md`, Inbox queue).
- `task down ID --json`, `task later ID --until TIME --json` and `task requeue ID --json` set or clear Move down and Later, and print the Task's item. `TIME` is `2026-11-04T09:00Z` (UTC), `+30m`, `+2h`, `+1d` or epoch seconds. They need the `task-inbox` capability; without it the command fails with `unsupported`.
- `task new --json` prints one item as soon as the record is saved. It then stays until the bridge has typed the start prompt, or gave up after 30 s. Read the first line; do not wait for the exit.
- `task restart ID [--force] --json` (ADR 0018) prints the restarted Task's item the same way.
- Task Loop commands (ADR 0019). Each needs the daemon on the host; without it the error code is `unsupported`, and the message says what to run.
  - `task new ... --check CMD [--max-attempts N] [--check-timeout S] --json` creates the Task with a loop.
  - `task loop start ID --check CMD [--max-attempts N] [--check-timeout S] [--reset] --json` starts the loop again, also a stopped or passed one. The attempt count goes on unless `--reset`. It prints the Task's item.
  - `task loop stop ID --json`, or `task stop-loop ID --json`, stops it with reason `user`, and prints the item.
  - `task stop-loop --all --json` stops every loop that is `armed`, `checking` or `paused`, and prints `{"tasks": [item, ...]}` for those it stopped. When one fails, the others still stop, and the command exits non-zero with each failed id on stderr.
  - `CMD` must be one the workspace's `[workspace.loop]` allows; otherwise the code is `bad-request`.
  - The item's `record.loop` is the loop as `protocol/PROTOCOL.md` (Task Loop) describes it. A task whose loop is `paused`, or `stopped` with a reason other than `user` or `status`, is in `needs-attention`.
- An item has:
  - `group`: `needs-attention`, `ready-for-review`, `running`, `no-agent` or `closed`. The rule is ADR 0015's, the same as in the apps (`crates/muxa-cli/src/inbox.rs`). Treat an unknown group as one without a known meaning.
  - `need`: `none`, `answer`, `agent-error`, `loop`, `review`, `no-agent` or `no-session` (ADR 0023). Running and Closed have `none`. Waiting and error take precedence over loop attention; loop attention takes precedence over review or missing agents. Unknown values have no known action.
  - `record`: the task record as `protocol/PROTOCOL.md` describes it.
  - `session`: optional, `{id, name}` of the session whose `taskId` is the record's id. Target it by `id`.
  - `agent`: optional, `{state, since?}` of that session's agent pane, only when it can be believed: not with a shell in the foreground, and not for a closed Task.
- Items come in group order (the order above), newest `updatedAt` first within a group.
- New fields can appear. Changing a name above is a breaking change.

### Device sessions

While a phone or tablet shows a pane, the bridge holds the pane's window in a session of its own (ADR 0021): a device session, named `_muxa-<pid>-<n>`, with the session option `@muxa_device` set to `1` and `@muxa_device_pid` set to the bridge's pid. It holds only that window, which stays in the user's session too.

- A desktop tool must skip sessions with `@muxa_device` in every list, or it shows the window twice. With tmux 3.1 or later: `list-sessions`, `list-windows -a` and `list-panes -a` with `-f '#{==:#{@muxa_device},}'`.
- A tool that closes a session must first kill the device sessions that hold its windows. Otherwise the window keeps running in the device session.
- A tool never kills or changes another device session: the bridge ends it when the device stops watching, and removes leftovers of a bridge that crashed.
- tmux-agents does all three (its README, "For other tools").

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

## Recovering an interrupted restart

Hosts advertising `task-restart-recovery` expose an unresolved operation in Task detail and CLI JSON. This capability is independent of named workers. Inspect first:

```sh
muxa-cli task recover t-000001 --json
```

Use the actual Task id, operation id, revision and offered action from that inspection. For example, a prepared operation can be explicitly cancelled:

```sh
muxa-cli task recover t-000001 --operation restart-example --rev 7 --action cancel-prepared
```

`cancel-prepared` preserves the previous process and live files. `finish-replacement` reconciles a durably confirmed replacement and staged files only while process/file identities still match. `leave-unconfirmed` acknowledges an uncertain restart or delivery and stops automatic work. None respawns, sends input or resumes a loop. The apps require confirmation and the same displayed identity. Unknown phases offer Terminal rather than an invented recovery action. Stale identities conflict; refresh deliberately instead of silently retrying with a newer revision.
