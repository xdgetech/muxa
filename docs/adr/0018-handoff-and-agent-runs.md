# 0018: Handoff and agent runs

## Status

Proposed.

## Context

- A long Task outlives one agent session. Context fills up, an agent gets stuck, or the user wants another agent. Today the only way on is to quit the agent in its pane and start a new one by hand. The new agent knows only what `.muxa/TASK.md` says: the goal (ADR 0015).
- An LLM summary written by the old agent is useful, but it can be wrong or missing. The bridge can add facts it can check: which agents ran and when, where their transcripts are, what git says, and which files changed.
- The record does not say which agents worked on a Task. Without that, a client cannot show a Task's history, and a new agent cannot find the last transcript.
- The bridge runs per SSH connection and has no daemon (ADR 0003, 0015).
- Invariant 2: the bridge never changes the size or layout of a window or pane, and never selects one. A restart must keep the pane where it is.

## Decision

### Agent runs in the record

The record gets an optional list `runs`. Each run:

| Field | Meaning |
|---|---|
| `id` | An integer from 1, one higher than the previous run's. Never reused, also after old runs are dropped. |
| `agent` | The adapter id that saw the agent, for example `claude-code` or `codex`. Absent until an adapter reports the agent. |
| `startedAt` | Epoch seconds when the bridge started or linked the agent. |
| `endedAt` | Epoch seconds when the bridge restarted it. Absent while it runs, or when the bridge did not see it end. |
| `agentSession` | The agent's own session id, when the adapter knows it: Claude Code's `sessionId`, Codex's rollout id. |
| `transcript` | The absolute path of that session's transcript. A path only, never content. |
| `reason` | `start` (`create-task`) or `restart` (`restart-agent`). |

- The bridge writes a run at `create-task`, after the session is linked, and at `restart-agent`.
- `agent`, `agentSession` and `transcript` are filled in when the adapter reports the agent in the Task's agent pane. That happens on the prompt path below, which already waits for the adapter. At a restart, the run that ends gets them again, because an agent can switch sessions (Claude Code's `/clear`).
- `agent` is optional, not required. At launch the bridge knows a command, not an agent. Writing a guess would break invariant 6.
- At most 50 runs are kept, the newest. A 50-run list is about 15 KiB, well inside the 64 KiB record cap.
- **Without a daemon, runs are only recorded while a bridge acts.** An agent that the user quits or restarts by hand in the pane, or that exits while no client is connected, leaves no run and no `endedAt`. `endedAt` means "ended by a muxa restart", not "the process ended".
- The change is additive: `runs` is a new optional field. Unknown fields already survive a rewrite (ADR 0015), so a bridge that does not know `runs` keeps it. The format stays `version` 1.

### `TASK.md` is the task's working memory

`.muxa/TASK.md` has two parts, split by one marker line:

```markdown
# <title>

## Goal

<goal>

## Context

- Task: t-7f3a2c
- Workspace: work
- Project: auth          (when set)
- Repo: /abs/repo        (when set)
- Branch: feat/login     (when set)

## How to use this file

Keep the sections below the marker current as you work: what is done, what you decided and why, and what comes next. A new agent session on this Task reads this file first. muxa rewrites only the part above the marker.

<!-- muxa:notes - muxa never changes anything below this line. -->

## Progress

Nothing yet.

## Decisions

Nothing yet.

## Next actions

Nothing yet.
```

- The bridge owns everything above the marker. The agent owns the marker's line and everything below it.
- `create-task` writes a new `TASK.md`, as before (ADR 0015). A branch whose worktree is reused gets the new Task's file.
- When `restart-agent` writes `TASK.md` again, it renders the header and keeps the old file from the marker line on, byte for byte. The marker is found as a line that starts with `<!-- muxa:notes`.
- A `TASK.md` without the marker was written before this ADR, or edited by hand. The bridge writes the new header and the default sections, and then keeps the whole old text under `## Earlier notes`. Nothing is lost.
- The sections are a request to the agent, not a check. The bridge never reads them.

### `.muxa/HANDOFF.md`, written before a restart

The bridge writes `.muxa/HANDOFF.md` in the Task's `dir` just before it restarts the agent. It replaces any earlier one. It holds facts the bridge can check:

1. The record's `status` and `note`.
2. The runs, oldest first: agent, start and end in UTC, the agent session id, and the transcript path. The run that ends now is included, with what the adapter reports for it at that moment.
3. For a Task with a repo:
   - the branch, and its commits since the fork point with its base: `git log --format='%h %s' <fork>..HEAD`, at most 50. The base is found as for `branch-changes` (`changes/git.rs`).
   - `git status --short`, at most 200 lines.
4. For a Task without a repo: the files in `dir` changed since the ending run started, by mtime, newest first, at most 100. `.muxa/` and hidden directories are skipped. The walk stops after 10 000 entries.

- No transcript content is copied. The transcript is the agent's to read; HANDOFF.md says where it is (invariant 5, in spirit). Git commit subjects and file names are repo metadata, written to a file in the Task's own directory, never to logs.
- `.muxa/` is already in git's exclude file (ADR 0015), so HANDOFF.md never shows as a change.

### `restart-agent {task, force?}`

1. The bridge reads the record and finds its linked session, by `@muxa_task` or by `dir` (ADR 0015). In that session it takes the first live pane whose role is `agent`.
2. While that pane's agent state is `running`, the request fails with `bad-request`, unless `force` is true. `waiting` and every other state are allowed: a stuck permission menu is a reason to restart.
3. It writes the header of `TASK.md` (above) and `HANDOFF.md`.
4. It restarts the agent in **the same pane**: `respawn-pane -k -t %n -c <dir> -e KEY=VALUE…`, through the quoting function, on the control client's stdin. The `-e` values are the Task workspace's `agent.env`, read from `workspaces.toml` on the request (ADR 0016).
   - When the pane has a start command (the built-in launcher), respawn-pane gets no command, so tmux reruns that same command.
   - When it has none (tmux-agents starts a shell and types the agent command into it), respawn-pane starts a fresh shell, and the bridge types the agent command into it: the workspace's `agent.command`, else `--task-command`. tmux-agents' own launch command is not read. That is a gap until ADR 0017 gives tmux-agents a contract for it.
5. It ends the previous run and adds a run with `reason: restart`.
6. It types `Read .muxa/TASK.md and .muxa/HANDOFF.md, then continue.` with the start prompt's code (ADR 0015): once the adapter reports `idle`, within 30 seconds, else a note says to type it.
   - The `idle` must be new: its `since` must be at or after the restart. A tmux-agents state tag survives respawn-pane, and an old `idle` would type the prompt into a starting agent. A state with no `since` is not new.
7. The client gets `task-saved` with the record, then `result`.

`scripts/repro/respawn-pane.sh` checks what this relies on: the pane id, the pane's size and the window layout stay; nothing is selected and no client moves; `-e` and `-c` reach the new process; the session's environment stays; a pane without a start command gets a fresh shell.

### Protocol

- Capability `task-handoff`: the bridge understands `restart-agent` and writes `runs`.
- Request `restart-agent {id, task, force?}`. A `task-saved` comes before the `result`.
- Record field `runs` (above), absent when empty.
- Errors: an unknown task or a running agent is `bad-request`. No linked session, no agent pane, or a failed respawn is `task-failed`.

## Consequences

- A Task can go on with a fresh agent, from the phone, and the new agent gets facts, not only a summary.
- Clients can show a Task's agent history from `runs`.
- The run history has gaps: anything done in the pane by hand, or while no bridge acts, is missing. A daemon (ADR 0015 lists its triggers) would close them.
- For tmux-agents tasks the restarted agent runs the bridge's agent command, not tmux-agents' launch command, until ADR 0017 settles it.
- Rejected: `kill-pane` and a new `split-window`. It changes the layout (invariant 2).
- Rejected: a new window or session for the new agent. The desktop layout would change, and the Task would have two agent panes.
- Rejected: copying the end of the transcript into HANDOFF.md. Transcripts are the agent's records; a path lets the new agent read exactly what it needs.
- Rejected: asking the old agent to write a summary before the restart. It may be stuck, which is often why the user restarts it. The agent keeps TASK.md's sections current instead, while it works.
- Rejected: bumping the record `version`. `runs` is additive, and old bridges keep it.
