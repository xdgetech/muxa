# 0015: Task records and workspaces

## Status

Proposed.

## Context

- Today a "task" is a tmux session with a worktree (`TaskSession`, `crates/muxa-bridge/src/tasks/mod.rs`, ADR 0007). No goal is recorded, a task needs a repo, and closing the session forgets it.
- Nothing groups work above the repo, and no view shows what needs the user across sessions. The banner of ADR 0008 is transient.
- The vision (`docs/vision.md`) needs a durable Task that outlives its session and can also hold work without a repo, such as product or admin work.
- The bridge runs once per SSH connection and has no daemon (ADR 0003). An iPad and an iPhone can each run a bridge at the same time.
- The wire already uses "task" for the worktree session: the requests `new-task` and `close-task`, and the session field `task`.

## Decision

### Names

- The domain word is **Task**. A **task record** is the durable Task.
- `new-task` and `close-task` keep their meaning: they start and close a session for work.
- The session field `task` keeps its meaning: tmux-agents' `@agent_task` (`crates/muxa-tmux/src/discover.rs`).
- New names avoid the collision: module `records/`, capability `task-records`, requests `list-tasks`, `create-task`, `update-task`, and the session field `taskId`.

### The record store

- One JSON file per Task in `$XDG_STATE_HOME/muxa/tasks/`, or `~/.local/state/muxa/tasks/` when the variable is unset. The directory has mode `0700`.
- Every write goes to a temp file in the same directory, then `rename`. A reader never sees half a record.
- A record has `version` (the file format) and `rev` (incremented on every write). Unknown fields are kept on rewrite.
- `update-task {task, rev, …}` fails with a new error, `conflict`, when `rev` is not the current one. The client reloads and tries again. Two bridges cannot overwrite each other's change.
- Ids are short and readable: `t-` and 6 hex digits (24 random bits), for example `t-7f3a2c`. A new id is checked against existing files. A short id can later be passed to tmux-agents' `new.sh --task`.
- `records/` does not depend on `server.rs`. A later daemon can use the same code.

### Workspaces

- Workspaces come from host config, `~/.config/muxa/workspaces.toml`: a name, a root directory, and optional repos per workspace.
- Clients cannot write this file. It decides which directories an agent may get, so it is a host decision, like `--agent-dir` (ADR 0005).
- Project is an optional free-text field on the Task. There is no Project entity yet.

### Starting a Task and linking its session

- `create-task` writes the record first, then starts the session.
- **With a repo**, the existing launcher path of ADR 0007 runs.
- **Without a repo**, the built-in launcher gets a directory mode. It makes `<workspace root>/tasks/<slug>-<id>/` and starts the session there.
- `TaskLauncher::launch` returns the `SessionId`, not the session name.
  - The built-in launcher has the id when it creates the session.
  - The tmux-agents launcher resolves it once by `@agent_worktree` equal to the worktree path, never by name (invariant 3).
- After launch, the bridge sets the session option `@muxa_task <id>` with the existing `set_session_option`. Discovery reads it, and the protocol sends it as the session field `taskId`.
- Both launchers return the existing session when the branch already has one. `create-task` refuses when that session is already linked to another open Task.
- Session options end with the tmux server. So the record also stores its `dir`: the task directory or the worktree. When no session carries the tag, the bridge matches `@agent_worktree`, or the first pane's path, against `dir`. This is the same rule `sessions.rs` uses to find a session's repo.
- A failed launch keeps the record, with no session and a short error note.

### The goal reaches the agent through `TASK.md`

- The bridge writes `.muxa/TASK.md` in the Task's `dir`: the goal, the workspace and the resources.
- In a worktree, `.muxa/` is added to git's exclude file, so it never shows as a change.
- The bridge then types one fixed prompt into the agent pane: `Read .muxa/TASK.md and start.` It uses the existing `send-keys -l` path. No user text goes on a command line, so this works the same for both launchers, including tmux-agents' own agent command.
- The prompt is typed only after the adapter reports the agent `idle` or `waiting` for input, with a timeout. Otherwise the bridge does not type it, and says so in the record's note.
- `TASK.md` is the first, minimal context handoff. A new agent session on the same Task reads the same file.
- The contract that hosts and agents rely on: the record format, the `@muxa_task` tag, and the path `.muxa/TASK.md`.

### Task status and agent status

- Task status is set by the user: `open`, `review`, `done`, `dropped`.
- Agent status comes from the adapters, as today (ADR 0008). The two are never merged.
- `close-task` ends the session. It does not change the Task's status.
- The Inbox is derived on the client, from `list-tasks` and the agent state of each linked session in `list`:

  | Group | Rule |
  |---|---|
  | Needs attention | agent `waiting` |
  | Ready for review | agent `idle` or `done`, and task `open` |
  | Running | agent `running` |
  | No agent | no linked session, no agent state, or `shellForeground: true` |

- Every state comes from an adapter (invariant 6). Nothing is guessed.

### Security

- Paths are canonicalized before the check that they are inside the workspace root. A symlink cannot lead out of it.
- The slug uses only `[a-z0-9-]`, at most 40 characters, followed by the id.
- Repo Tasks keep today's rule: any absolute path inside a git repo (`tasks/mod.rs`, `SECURITY.md`). Limiting repos to workspace roots is a separate, later decision.
- Logs and error messages never contain a Task's title or goal. Invariant 5 now covers Task content.
- No new trust boundary. The client's SSH key can already type into any pane, so writing `TASK.md` and typing a fixed prompt give it nothing new (ADR 0005 says the same for transcripts).

### No daemon now

- The per-connection bridge reads and writes the records (ADR 0003 stands). Nothing in this slice has to run while no client is connected.
- A daemon gets its own ADR when one of these is needed:
  - the Task loop starts or continues work while no client is connected;
  - a workflow step must start when another one ends;
  - push notifications while the app is closed (ADR 0008).
- `rev` already gives a single-writer daemon a clear ownership rule, without a migration.

## Consequences

- The record format becomes a contract once hosts hold records. `version` exists from day one, and unknown fields survive a rewrite.
- A Task can exist without a session, and without a repo.
- The Inbox needs no new message. Old clients keep working; they ignore `taskId`.
- After a tmux server restart, the link is found again through `dir`, but only for sessions started in that directory.
- Tests write records to a temp directory and run on throwaway `-L` servers, with the built-in launcher and a fake `new.sh`.
- Rejected: storing the Task only as tmux options. They end with the server and cannot hold the goal.
- Rejected: a daemon now. Nothing in this slice runs while no client is connected.
- Rejected: SQLite now. One small JSON file per Task is enough for this slice, and a person can read it.
- Rejected: renaming `new-task`, `close-task` and the field `task`. Existing clients use them.
- Rejected: clients writing `workspaces.toml`. It decides what an agent can reach, so it belongs to the host.
