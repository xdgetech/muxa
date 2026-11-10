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
- The worktree and naming rules belong to tmux-agents, and `builtin.rs` copies them. `crates/muxa-bridge/tests/tmux_agents.rs` runs both on the same branches and fails when they differ: `make test-tmux-plugin` runs it against `tmux/` (ADR 0032).
- `builtin.rs` makes the worktree with git and a detached session with the agent command (`--task-command`, default `claude`). It is picked otherwise, or with `--task-launcher builtin`.
- `close` closes a task's session and removes its worktree as the mode allows, never one with uncommitted changes. `tmux_agents.rs` runs `close.sh`; `builtin.rs` checks the same things itself.
- A workspace's `[workspace.agent]` (ADR 0016) sets the agent command and environment. The built-in launcher passes the environment as `new-session -e`, on the control client's stdin; only keys are logged. tmux-agents cannot pass it yet, so a Task whose workspace has that table always uses the built-in launcher. A `new-task` without a workspace takes the table of the one workspace that has the repo: listed in its `repos`, or under its root.
- `Builtin::launch_in_dir` is the directory mode for a task record without a repo (ADR 0015). It makes `<workspace root>/tasks/<slug>-<id>/` and a detached session of that name running `--task-command`, with the pane tagged `@muxa_role agent`. It is used even when tmux-agents is installed: `new.sh` needs a repo.
- A workspace's repos (ADR 0031, `records/repos.rs`) are the git repos one or two levels under its root, plus its `repos`. The scan skips symlinks, dot-folders, `tasks/`, `routines/` and the inside of a repo, and stops after 2000 entries or 250 ms. `create-task` and `start-session` take a repo relative to the root; it must stay inside the root after canonicalization, unless it reaches a listed repo.
- A repo Task without a branch gets one from its title (`tasks/branch.rs`): the slug, then `-2`, `-3`... while that local or `origin` branch, its worktree folder or the main checkout's branch exists. Both launchers get the derived branch, so tmux-agents names the worktree and session as for any branch.
- `start-session` starts an ask session (`tasks/ask.rs`): `ask-<repo>-<n>`, detached, in the repo's main checkout, with the workspace's `[workspace.agent]`, through `Builtin::start`. The session gets `@muxa_ask <workspace>`, never `@muxa_task`, and no record, so it lists as a manual session. tmux-agents is not used: there is no worktree to make. `close-task` closes it with `ask::close` (ADR 0039): only a session with `@muxa_ask` and no worktree or `@muxa_task`, through the same `tasks::end_session` as the built-in launcher's close. The main checkout is never touched.
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
- **What the Inbox needs from an adapter** (ADR 0033, `src/items/produce.rs`). The same state and `since`, for each open Task with a session. `waiting` makes an `ask` with the adapter's Chat prompt, a finished turn (`done`, or `idle` after a reply in the transcript) a `review`, and `error` a `failure`. While a loop is active, only its pass or stop makes a `review` or `failure`. `running` resolves the open item. Each need, `<kind>:<since>`, makes one item, whether the daemon or a bridge sees it first. The item stores the last reply, at most 4 KiB, and counts of the pane's changes. A state with no `since` makes nothing. The daemon looks every 2 s while an open Task has a session, else every 5 minutes.
- **Typing.** The loop types its fixed prompt with `send-keys -l` and Enter, only on a finished turn, and only when no desktop client on the Task's session was active in the last 5 s (`loops/typing.rs`). It never sends menu keys.
- **The adapters' directories** come from the daemon's own flags and `workspaces.toml`, read when it starts. Restart the daemon after a change to `[workspace.agent]`.

## Routines

A Routine (ADR 0026, `src/routines/`) is a `[[routine]]` in a workspace's `<root>/.muxa/routines.toml` that the daemon runs on its schedule, or on `run-routine`. The host allows a workspace's Routines, and their direct writes, with `[workspace.routines]` in `workspaces.toml` (ADR 0036). A broken workspace file hides only that workspace's Routines; `routine-list` reports it in `problems`. Each run is a task record of kind `routine-run`: the same create path, session tag, runs, delivery claims and Loop engine as a Task. `list-tasks` skips it.

- **Where it runs.** A new directory `<root>/routines/<name>-<id>` in the Routine's workspace, in a session tagged `@muxa_task`, so it is not a manual session. Its `.muxa/TASK.md` holds the Routine's instructions and a fixed Output section.
- **After it publishes** (ADR 0037). The daemon closes the run's session on its next tick. The folder stays until the Routine's next run publishes, then goes. A failed run keeps both.
- **How the agent writes output.** It writes `.muxa/routine/output.json` (the schema is in `protocol/PROTOCOL.md`, "Routine output"), then ends its turn. The state the last successful run committed is in `.muxa/routine/state.json`. The folder is the run's private stage; nothing outside the run reads it before the check.
- **The check.** The Loop's check for this kind runs in the daemon's process, not as a shell command (`routines/check.rs`). It validates kinds, the item limit, sizes and links. On a pass, one locked write of `<state dir>/routines/r-<hash>.json` stores the new entries, skips keys the Routine already stored, commits the state and closes the run; the record becomes `done`. On a fail, the reasons go to `.muxa/checks/<attempt>.log`, and the loop types: `The Routine output did not pass the check. The reasons are in .muxa/checks/<attempt>.log. Fix .muxa/routine/output.json, then end your turn.` After 3 fails, the run is dropped and the Routine shows the failure. When the agent does not become idle within 30 s, for example on a folder trust dialog, the start prompt is not typed: the run fails the same way, keeps its session, and the agent never gets the fail prompt.
- **What needs an adapter.** The same as the Task Loop: a finished turn with its `since`. A Routine has no person to type the start prompt, so it needs automatic delivery. Codex does not have it yet (ADR 0028); a Routine whose worker or default command is Codex is refused before it starts.
- **Approval.** `update-inbox approve` creates one Task through `create-task`'s path. Its goal holds the proposal unchanged, its hash, and a last line `muxa-proposal: <entry> <hash>`. A second approval, also after a crash, finds that Task by the line instead of making another.
- **Logs.** Ids, counts and outcomes only. Never instructions, titles, bodies, links or state.

## Assistant runs

An assistant request (ADR 0048, `src/assistant/`) asks one workspace's agent, its delegate, to work on a Work item. Each run is a task record of kind `assistant-run`, with the same create path, session tag, delivery claims and Loop engine as a Routine run. `list-tasks`, the Inbox items and park skip it.

- **Configuration.** In `workspaces.toml` (`docs/examples/workspaces.toml`):
  - `[assistant]` at the top level is the host's switch: `enabled`, and `max_runs_per_day` across all workspaces (default 20). Without it, no workspace takes requests.
  - `[workspace.assistant]` allows requests to one workspace: `delegate = true`, an optional `worker`, `max_open_runs` (default 1), `max_runs_per_day` (default 10) and `run_timeout` (default `"30m"`). Without it, the workspace keeps Captures and Work items but takes no requests.
- **Claude Code only.** The delegate is `[workspace.assistant].worker`, else the workspace's agent. A request needs automatic delivery and a finished turn. A Codex delegate (ADR 0028), or a command with no muxa adapter, is refused before any run folder exists, and the message says to set a Claude Code worker.
- **Budgets.** Before a run starts: the workspace's open runs stay within `max_open_runs`, and the runs started today within both `max_runs_per_day`. A refusal is `conflict` with the limit and when it resets.
- **The daemon.** Requests run only in the daemon. Without it, `request-assistant` is `unsupported` and clients say automatic processing is off. Captures and lists work in the bridge alone.
- **Where it runs.** A new directory `<root>/assistant/<scope>-<request>-v<version>-<id>` in the workspace, and a session of the same name tagged `@muxa_task`. Neither name holds Capture text. The goal ends with `muxa-request: <request> v<version>`; after a crash, one request version still makes one run.
- **The brief.** `.muxa/TASK.md` holds the Work item's title, the scope (`research`, `compare` or `draft`) and its rule, the purpose, the linked Captures as written, and Earlier work: the request's earlier Results and the Tasks started for the item, newest first. Captures and earlier summaries are marked as data, not instructions. The Result section states the schema below.
- **The result file.** The agent writes `.muxa/assistant/result.json`, then ends its turn. The schema and limits are in `protocol/PROTOCOL.md`, "Assistant result file".
- **The check.** It runs in the daemon's process (`assistant/result.rs`). On a pass, one locked write stores the Result as a new version on the request; older Results become `stale`. The Work item does not change. On a fail, the reasons go to `.muxa/checks/<attempt>.log`, and the loop types: `The assistant result did not pass the check. The reasons are in .muxa/checks/<attempt>.log. Fix .muxa/assistant/result.json, then end your turn.` After 3 fails, the request is `failed`.
- **After the run.** A stored Result closes the session on the daemon's next tick. The folder stays until the next version stores a Result, or for 7 days. After `run_timeout` with no Result, the request is `timed-out`, but the agent is not stopped, and a later Result is still stored. `cancel-request` and a new version end the older run.
- **Logs.** Ids, sizes and states only. Never Capture text, a brief or a Result.

### Delegate notes (ADR 0049 §1)

Each delegate keeps `<root>/.muxa/assistant/NOTES.md`: durable facts about the workspace's work, the user's stated preferences for it, and where things are. The brief tells the agent what never goes there: Task progress, whole Capture texts, secrets, and anything about another workspace.

- **The copy.** At a run's start the host copies the live file to `.muxa/assistant/NOTES.md` in the run folder and keeps its SHA-256 as `base`. A live file over 16 KiB, not UTF-8, with a NUL byte, a symlink or not a regular file fails the start with its path and size, before any session exists.
- **Proposing.** The agent never writes the live file. It puts the full new text in `notes` of `result.json` (at most 16 KiB). The host stores it on the Result as `notes {text, base}`. The text the run read proposes nothing.
- **Applying.** `[workspace.assistant] notes` decides who writes it. `"review"` (the default): the user reads the proposal in Work detail and taps Update notes, which sends `apply-notes {source, version, base}`. `"auto"`: the host applies a stored proposal at once. Both write only while the live file still hashes to `base`; else `apply-notes` is `conflict` and `auto` keeps the proposal for the user.
- **Forgetting.** Edit or delete the file. The next run reads the edited file. Stored Results keep their `notes` as history.

### The overall assistant (ADR 0049 §2–§5)

The overall assistant is a workspace with `[workspace.assistant] overall = true`, at most one per host. It uses the workspace's own root, agent, env, worker and budgets. Its root holds only its notes and run folders. Clients leave it out of the New Task and Capture pickers, and it takes no requests (`unsupported`).

- **Configuration** (`docs/examples/workspaces.toml`):
  - On each other workspace with `[workspace.assistant]`: `share` (`none`, `outline` or `summaries`; default `summaries` with `delegate = true`, else `none`) says what the overall assistant reads. `overall_requests` (`none`, `review` or `auto`; default `review`) says whether it may ask that workspace.
  - On the overall workspace: `plans`, at most 3 schedules of five cron fields (default `["30 7 * * 1-5"]`, weekdays at 07:30; `[]` plans on demand only), and `timezone`, an IANA name (default the host's zone). `delegate`, `share` and `overall_requests` are refused there. Its budget defaults are `max_open_runs` 1, `max_runs_per_day` 8 and `run_timeout` `"15m"`.
  - `[assistant] max_auto_requests_per_day` (default 5) caps the requests it sends without approval, across the host. Its runs also count against `[assistant] max_runs_per_day`.
- **Triggers.** Each schedule runs as a Routine's: one catch-up after the host slept, a repeated DST hour once. Schedules due at once make one plan. `plan-now {minutes?}` starts one on demand. One plan run is open at a time; a second is `conflict`. A reached budget keeps the occurrence as a `failed` plan with its `reason`, and the next occurrence runs again.
- **The brief.** The host writes it to `.muxa/assistant/BRIEF.md` in the run folder (mode 0600), and the goal points to it. Per workspace, by `share`:
  - `outline`: Work item titles, states, `laterAt`, `pinnedAt` and `due`; request scopes and delivery states with their times; open Inbox items' kinds, titles and ages.
  - `summaries`: `outline`, plus each Work item's newest current Result: the summary cut to 600 characters, at most 3 open questions and 3 next actions of 200 characters each, and the number of findings.
  - `none`: nothing, not even the name.
  - Also: unsorted Captures and Work items without a workspace, whole, because sorting them is its job; the open Inbox items, the previous plan, its own notes and the plan schema. The shared part is at most 32 KiB; the oldest items are left out first, and the brief says how many.
  - Never: findings' text, sources and file paths, Result `files`, a workspace's Capture text, a delegate's notes, `TASK.md`, or pane content.
- **The plan file.** The agent writes `.muxa/assistant/plan.json` and ends its turn: at most 3 `top` rows, `proposals[]` and optional `notes`. The host checks it in its own process, as a request's result, and enforces the rules the agent cannot be trusted with: pinned Work items first in pin order, no item with `laterAt` in `top`. It derives `dueSoon`, `unanswered`, `stale` and `changed` itself. The schema and limits are in `protocol/PROTOCOL.md`, "Plans and the overall assistant" and "Plan file". The host keeps 7 plans.
- **Proposals.** Each `sort` or `delegate` proposal of the newest plan becomes one Inbox item with `approve`, `reject` and `later`. A proposal makes no item when an open item already asks the same thing. `approve` runs `sort-capture` or `request-assistant` once; a second approve returns what the first made. A `delegate` to a workspace with `overall_requests = "auto"` is approved by the host itself, within every budget and at most once per workspace per plan; the item stays, `answered`, so the user sees every request sent. Past a budget it stays open for the user. A `sort` is always reviewed. A `none` workspace refuses even an approved request.
- **Logs.** Ids, counts, hashes, states and schedule times. Never notes, plan texts, reasons, purposes or summaries.

From a terminal: `muxa-cli routines`, `routine run [WORKSPACE/]NAME`, `inbox`, and `inbox approve|dismiss ENTRY` or `inbox later ENTRY --until +4h`. With `work` level 3, `inbox` lists Inbox items, and `inbox answer ITEM ACTION` answers one (Desktop tools below).

## Desktop tools

A tool on the host, such as tmux-agents (ADR 0017), reads and creates Tasks through `muxa-cli` with `--json`. It never reads or writes the record files itself.

```sh
muxa-cli --bridge 'muxa-bridge [--socket S]' tasks --json
muxa-cli --bridge 'muxa-bridge [--socket S]' task new --workspace W --title T --goal G \
  [--repo R [--branch B] [--base REF]] [--project P] --json
muxa-cli --bridge 'muxa-bridge [--socket S]' ask REPO [--workspace W] --json
```

- Each prints one line of JSON on stdout. A failure prints `muxa-cli: <code>: <message>` on stderr and exits non-zero.
- `tasks --json` prints `{"workspaces": [names], "workspaceInfo": [{name, root?, checks?, repos?}], "capabilities": [...], "truncated": bool, "tasks": [item, ...]}`. `workspaces` stays a list of names for older desktop clients; `workspaceInfo` adds each workspace's root and repos (ADR 0031), and `capabilities` are the host's, so a client can check for `work:2`.
- `task done ID --json` and `task drop ID --json` set the status to `done` or `dropped` and print the Task's item. Both close the Task's session and remove its worktree and branch (ADR 0043). When that could lose work they change nothing and fail with `would-lose-work: <reason>`; `--force` removes them anyway.
- `inbox --json` prints `{"work": N, "items": [item, ...], "entries": [entry, ...], "runs": [...]}`. `work` is the host's `work` level, so a tool can tell an empty Inbox from a host without items. `items` are the Inbox items of ADR 0033, with `work` level 3 (`protocol/PROTOCOL.md`, Inbox items); without it `items` is empty. `entries` and `runs` stay for older tools. With `work` level 9 it also has `workItems`, `captures` and `requests` (ADR 0048): the Work items, without `done` and `withdrawn` ones unless `--all`, the unsorted Captures, and the assistant requests of the listed items. With `work` level 10 it also has `plans` (the kept plans, newest first) and `assistant` (`{overall?, delegates[]}`, or `null`) (ADR 0049). With `work` level 11 it also has `policies` (active, then expired), `hypotheses` (`proposed` and `rejected`, newest first) and `corrections` (newest first, at most 50) (ADR 0050). The text form lists `sort`, `delegate` and `hypothesis` items as any other item; `inbox answer ITEM approve|reject|later` answers them, and an approve prints the answered item. Without `--all`, only `open` and `later` items and entries are listed. The text form, with level 3, prints one line per item: id, kind, state (`unread` when not read), age, title and question.
- `inbox answer ITEM ACTION [--text T] [--option N] [--until TIME] --json` sends `update-inbox` and prints the saved item. `ACTION` is the wire name: `approve`, `reject`, `choose`, `accept`, `keep-open`, `reply`, `redo`, `change-course`, `retry`, `drop`, `dismiss`, `later` or `read`. `reply`, `redo` and `change-course` need `--text`, `choose` needs `--option`, `later` needs `--until` (`2026-11-04T09:00Z` (UTC), `+30m`, `+2h`, `+1d` or epoch seconds); each flag is only for its action, else the command exits 2 without a request. `inbox read ITEM` is `inbox answer ITEM read`. It needs `work` level 3.
- `capture TEXT [--workspace W] [--source app|tmux|cli] [--client-id ID] --json` saves TEXT (`-`: stdin) as a Capture and prints `{"capture": capture, "work": null}`. It starts nothing. The same `--client-id` returns the Capture already saved. `work [--all] --json` prints `{"work": [item, ...], "captures": [...]}`. `work show WORK --json` sends `get-work` and prints `{"work": item, "captures": [...], "requests": [...]}`: every Capture the item links, sorted or not, with its full text. The text form prints the item's line, each Capture's text (control characters removed, except line breaks and tabs), its Tasks and its newest request. `work sort CAPTURE (new [--title T] [--workspace W] | link WORK | note)` and `work (edit | later | done | withdraw | reopen | merge) WORK` read the record's `rev` first and send it; a `conflict` means it changed meanwhile. `task new ... --work WORK` starts the Task for that Work item, once per item `rev`. Each needs `work` level 9.
- `assist WORK [--scope research|compare|draft] [--purpose TEXT] [--workspace W] --json` reads the item's `rev`, sends `request-assistant` and prints `{"work": item, "request": request}`; the text form prints `q-… v2 research  pending  [work]`. `--scope` defaults to `research`, `--purpose -` reads stdin, and the workspace defaults to the item's. Asking again adds a version and ends the older version's run. `assist cancel REQUEST` sends `cancel-request` with the request's current version. `capture TEXT --assist --workspace W [--scope S] [--purpose P]` saves the Capture and asks in one `capture`; its JSON also has `request`. `assist show WORK|REQUEST` prints the newest Result to stdout: the summary, findings with their sources, open questions, next steps, files and possible duplicates, marked stale when it is an older version's; `--json` prints the request. `work` shows an asked item's newest delivery (`asked: processing`), and `work --json` has `requests`. The host's refusals are printed as it sends them, with exit 1: `unsupported` when automatic processing is off (no daemon), `conflict` with the limit and when it resets. When `capture --assist` is refused, the Capture is kept and the message says so. Each needs `work` level 9.
- With `work` level 10 (ADR 0049): `plan --json` prints the newest plan, or `null`. The text form prints the newest plan with a result: one line per row (the Work item or Inbox item id, its title when listed, the minutes, and `new` or `moved`), its why and next step, then `Due soon, moved down by you`, `Left the plan`, `Unanswered requests` and `Stale results`. A newer plan without a result comes first, with its delivery and reason. `plan now [--minutes N] --json` sends `plan-now` and prints the new plan; without an overall workspace or the daemon the host answers `unsupported`, while a plan is being made `conflict`. `work (pin | unpin) WORK` and `work due WORK --at TIME [--timezone ZONE]` read the item's `rev` and send `update-work` (`due` with action `edit`). TIME is RFC 3339 or `YYYY-MM-DD HH:MM` in ZONE, an IANA name (default: the system's). `assist notes WORK|REQUEST|PLAN` prints the newest proposed `NOTES.md` to stdout; `--apply` sends `apply-notes` with its source, version and base, and a changed live file is `conflict`. `--json` prints `{"source", "version", "notes"}`, or with `--apply` the saved request or plan. Against an older host each fails with "update muxa-bridge" and sends nothing.
- With `work` level 11 (ADR 0050): `policy --json` prints `{"policies", "hypotheses", "corrections"}`; the text form lists the policies, active then expired, then the hypotheses and the recent corrections, one line each. `policy add TEXT [--scope all|WORKSPACE] [--from TIME] [--until TIME|week] [--timezone ZONE]` sends `add-policy` (TEXT `-` reads stdin; `--scope` defaults to `all`; `--until week` ends this Sunday at 23:59 in ZONE) and prints the saved policy. `policy edit ID [--text T] [--scope S] [--from TIME] [--until TIME|week]` and `policy delete ID` read the policy's `rev` and send `update-policy`. `plan reorder ID...` names each row of the newest plan with a result once, in the user's order; `plan dismiss ROW [--reason T]` takes a row off Today. Both read the plan's `rev`, send `update-plan` and print the saved plan; the text form of `plan` follows `userOrder`, shows a dismissed row as one `not today` line, and lists each row's `policy`, `hypothesis` and `correction` basis as `Based on:` with the record's text, or `forgotten` when it is gone. `forget ID` sends `forget` for a correction `x-…` or a hypothesis `h-…`. `inbox answer ITEM confirm` makes a `hypothesis` item's text a policy; `inbox answer ITEM reject --reason T` keeps T as the correction's reason on `sort`, `delegate` and `hypothesis` items. A policy text is one paragraph of 1 to 500 bytes and a reason at most 500 bytes; a wrong one exits 2 without a request. Against an older host each fails with "update muxa-bridge" and sends nothing.
- `task new --json` prints one item as soon as the record is saved. It then stays until the bridge has typed the start prompt, or gave up after 30 s. Read the first line; do not wait for the exit.
- `task restart ID [--force] --json` (ADR 0018) prints the restarted Task's item the same way.
- With `work` level 2 (ADR 0031), `--repo` may be relative to the workspace root, for example `fairpull/fairpull-plan`, and `--branch` may be left out: the host names a new branch after the title.
- `ask REPO --json` starts the workspace's agent in the repo's main checkout, with no Task and no worktree, and prints `{"session": "$n", "name": "ask-<repo>-<n>", "workspace": W}`. Without `--workspace` it picks the only workspace, or the one whose repos hold `REPO`. It needs `work` level 2. The agent can change files in the main checkout.
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
  - a permission: `1` allows, `Escape` denies and interrupts the turn. The input box then takes a new prompt, which is how an Inbox `reply` on an `ask` sends its text (ADR 0033 §4a). No hook runs on that interrupt, so the pane's `@agent_state` stays `waiting` until the next prompt; the bridge waits for the adapter itself to report the menu gone and the session `idle` (at most 10 s) before it pastes;
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
