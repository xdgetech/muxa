# Agents

How muxa starts agents, follows what they report, and reads what they record. Everything here runs in the daemon (`crates/muxa-bridge`); the protocol and the apps never see an agent's own format.

## Starting an agent

Every agent is a herdr-core terminal. muxa finds its own terminals by their `muxa.*` labels (`protocol/PROTOCOL-v2.md`).

- **A Task** (`src/v2/tasks.rs`, `places.rs`, ADR 0068): the places are made first (a new folder `<root>/tasks/<slug>-<id>`, worktrees under `.worktrees/` next to each repo, read-only repos as they are), then the Workspace's agent starts in the working place. `.muxa/TASK.md` holds the title, the goal and every place; `.muxa/` goes into the repo's `info/exclude`. Claude Code also gets `--settings .muxa/claude-settings.json`, with the other places as additional directories and edits denied in the read-only ones, and `--permission-mode plan` when the Task asks for plan mode.
- **An adopted place** (`existing`, ADR 0076, `src/v2/adopt.rs`): a folder in the Workspace's root or a linked worktree of one of its repos, resolved with symlinks. Nothing is made; `.muxa/TASK.md` is written only when missing. A worktree on a branch is a branch place, so Approve closes it. The start is refused when a `claude` or `codex` process already works there: `ps` names the processes, and their working directory comes from `/proc` or `lsof` as for Codex Chat below. Claude Code gets `--continue` when `projects/<folder>/` in the Workspace's `CLAUDE_CONFIG_DIR` (else the daemon's Claude Code dirs) holds a `.jsonl` conversation.
- **Migration from v1** (`src/v2/v1.rs`): `migrate-v1` reads `<state dir>/t-<id>.json` (camelCase; `status`, `workspace`, `title`, `dir`), at most 64 KiB each, never writing one. A record that does not parse is logged by id and position only.
- **MCP servers** (`src/v2/mcp.rs`, ADR 0069, 0075): In a Workspace that lists any `[[workspace.mcp]]`, Claude Code gets `--mcp-config .muxa/mcp.json --strict-mcp-config` (an absolute path); in one without, it gets no MCP flags and keeps its own login's servers. The file lists every `[[workspace.mcp]]` server of the Workspace and no others, so user, project and plugin servers do not load. `start-task {mcp}`, `mcp` in `routines.toml` and `default` are ignored. A server's connections reach the agent's environment, and the file refers to them as `${VAR}`, which Claude Code expands (`docs/findings/claude-code-mcp.md`). Codex and other agents get no MCP servers yet; a Codex Task or Routine run in a Workspace with servers is refused before it starts.
- **The agent command** is the Workspace's `[workspace.agent] command`, else `--task-command` (default `claude`), with `[workspace.agent] env` and every connection of the Workspace (ADR 0016, 0069, 0075). Secrets come from the Keychain, service `muxa`, account `<workspace>/<connection>`, and go only into that agent's environment. A person sets the command, the env and the connections in `workspaces.toml` by hand or from an app's Settings (`settings-update`, ADR 0074).
- **The login** (ADR 0091, `src/records/subscriptions.rs`): `[[subscription]]` in `workspaces.toml` names an agent login (a name, `claude-code` or `codex`, its folder, a plan label). A Workspace, a Bot (`subscriptions` on `[[bot]]`, only from its Workspace's list) and the Capture assistant (`[assistant] subscriptions`) list them in order. An agent takes the first one of its kind (from its program: `claude` or `codex`) and gets its `CLAUDE_CONFIG_DIR` or `CODEX_HOME`, none for the default `~/.claude` or `~/.codex`, and the `muxa.subscription` label. A restart keeps the label's login while it exists; an adopted place uses the login whose folder holds its conversation. A Workspace with `subscriptions` may not also set those variables in its env. A name in `routines.toml` or `[assistant]` that is unknown or not allowed is skipped with a warning. The adapters search every subscription folder; the daemon reads the list again after each `settings-update`. Settings add, change and remove subscriptions (`subscription-add`, `subscription-update`, `subscription-remove`) and set the lists (`workspace-update`, `bot-update`, `assistant-update`). Usage (ADR 0091 §6, `src/limits.rs`, `src/status_line.rs`): a Claude Code agent with a subscription gets `statusLine` in its `.muxa/claude-settings.json` (merged into the file muxa already passes, else a new one; an agent command with its own `--settings` is left alone). Claude Code runs `muxa-bridge statusline --subscription NAME --state-dir DIR`, which records `rate_limits` and then runs and prints the login's own status line from its `settings.json`. Every minute the daemon reads the `token_count` events at the end of each Codex subscription's newest log, when it changed in the last 15 minutes. Both go to `<state dir>/subscriptions/<name>.json`, checked as untrusted: percentages 0 to 100, resets at most 8 days away, and an older reset than the stored one dropped. `subscriptions` serves them. Switching near a limit is not built yet.
- **A Routine run** (`src/v2/runner.rs`): the same, in `<root>/routines/<name>-<id>`, with the Routine's worker (a Workspace's `[[worker]]`) or the Workspace's agent, all the Workspace's connections and MCP servers, and the start prompt as its last argument.

## What an agent reports

The only agent state muxa uses is what the program reports itself with OSC 7501 (`working`, `blocked`, `waiting`, `done`, `idle`), which herdr-core keeps as the terminal's status. Claude Code reports it by itself. muxa never guesses a state from the screen (AGENTS.md, invariant 6).

- A finished turn is `done` (or `idle`). Task Loops and Routine runs act on it.
- `blocked` and `waiting` mean the agent waits on the user. **Needs you** lists those agents; a Task Loop pauses.
- An agent that ends with a non-zero exit, a crash or a lost program makes one Failed item (`muxa-server`'s `watch_tasks`). The user's own kill and a clean exit do not.

## Task Loops

ADR 0019, `src/v2/task_loop.rs`. After each finished turn, the check runs in the working place (`src/loops/check.rs`):

- `/bin/sh -c` in its own process group, with the daemon's environment and the Workspace's `[workspace.agent] env` on top, killed at the loop's timeout.
- Its output goes to `.muxa/checks/<attempt>.log`, and a summary with the end of the output to `.muxa/CHECK.md`. The last 20 logs are kept.
- A pass makes one Review item. A fail types `The check failed. Its output is in .muxa/checks/<n>.log. Fix the cause, then end your turn.` and Enter into the terminal; the last allowed fail makes a Failed item.
- The loop lives in `.muxa/loop.json`. A check that a daemon restart cut off is armed again.
- The check must be allowed by `[workspace.loop] checks` in `workspaces.toml`: `"any"`, or a list of exact commands. Neither the command nor its output reaches the log.

## Routines

A Routine (ADR 0026, 0036, `src/routines/`) is a `[[routine]]` in a workspace's `<root>/.muxa/routines.toml` that the daemon runs on its schedule, or on `run-routine`. `[workspace.routines]` in `workspaces.toml` allows a workspace's Routines. A broken routines file hides only that workspace's Routines; `routines` reports it in `problems`.

- **When.** Every 30 s the daemon starts the due runs: at most one per tick for the latest due occurrence (one catch-up), never two open runs of one Routine. DST gaps and repeats follow the Routine's time zone.
- **Where.** A new folder `<root>/routines/<name>-<id>`, as a terminal labelled `muxa.kind = routine`, `muxa.run`, `muxa.routine` and `muxa.occurrence`. Its `.muxa/TASK.md` holds the Routine's instructions and a fixed Output section, which ends with muxa's guide to good Inbox items (`src/routines/inbox_items.md`, for every Routine, with or without a Bot). The folder stays until the Routine's next run publishes; a failed run keeps its folder.
- **Output.** The agent writes `.muxa/routine/output.json` (below), then ends its turn. The state the last successful run committed is in `.muxa/routine/state.json`. The folder is the run's private stage; nothing outside the run reads it before the check.
- **The check** runs in the daemon's process on each `done` (`routines/check.rs`). On a pass, one locked write of `<state dir>/routines/r-<hash>.json` stores the new entries, skips keys the Routine already stored, commits the state and closes the run; the runner ends the terminal. On a fail, the reasons go to `.muxa/checks/<attempt>.log`, and the runner types `The Routine output did not pass the check. The reasons are in .muxa/checks/<attempt>.log. Fix .muxa/routine/output.json, then end your turn.` After 3 fails, the run fails and the Routine shows the failure. A run fails too when its agent ends without a publish, is gone, or runs past 2 h.
- **The Inbox.** `src/v2/feed.rs` copies new briefings as Read items, proposals as Decide items, replies as Reply items, to-dos as To do items and failures as Failed items, every 15 s, with their links, proposal and due date. A Bot's items count against its `daily_items` (ADR 0082).
- **Codex.** A Routine whose worker or default command is Codex is refused before it starts. A run acts on `done`, which only OSC 7501 reports, and whether Codex sends it by itself is unchecked (`docs/findings/session-backends.md`).
- **Logs.** Ids, counts and outcomes only. Never instructions, titles, bodies, links or state.

### Routine output

```json
{
  "version": 1,
  "items": [
    {"kind": "briefing", "key": "slack:C01:1762150000", "title": "Release thread",
     "body": "The release moved to Thursday.",
     "links": [{"url": "https://example.slack.com/archives/C01/p1762150000", "title": "Thread"}]},
    {"kind": "proposal", "key": "linear:ENG-12:stale", "title": "Close the stale issue ENG-12",
     "body": "No activity for 30 days.", "action": "Close the issue", "target": "linear:ENG-12",
     "content": "Closing: no activity for 30 days."}
  ],
  "state": {"cursor": "1762150000"},
  "memory": "Ana owns the release. The team ships on Thursdays."
}
```

- `version` is 1. `items` holds at most the Routine's item limit (default 10, at most 50).
- `key`: 1 to 200 bytes, no control character, unique in the file. A key the Routine already stored is skipped, so a retry stores nothing twice.
- `title`: 1 to 200 characters, one line. `body`: at most 4096 bytes; line breaks and tabs are the only control characters.
- `links`: at most 10. `url` is `https` or `http`, at most 2048 bytes, without spaces; `title` at most 200 characters.
- `kind` is `briefing` (or `read`), `proposal` (or `decide`), `reply` or `todo` (ADR 0082). Any item may have `due`, a date `YYYY-MM-DD`.
- A proposal also has `action` (1 to 200 characters, one line), `target` (1 to 500 bytes, one line) and `content` (at most 8192 bytes). Other kinds have none of them.
- `memory` is only for a Bot's Routine: the Bot's whole new `memory.md`, at most 16 KiB. The daemon writes it after the publish and keeps the five before it in `.muxa/bots/<name>/history/`. Absent keeps the memory.
- `state` is any JSON value, at most 16 KiB encoded. Absent keeps the previous state; it advances only with a successful publish.
- The whole file is at most 256 KiB, a regular file, not a symlink. Unknown fields are refused, so a typo is not lost silently.
- `handoffs` is only for a team lead's Routine (ADR 0085): `[{"to": "<member>", "brief": "…"}]`, one per member, a brief at most 4 KiB, never in the lead's last step (3 per occurrence). After the publish the daemon starts each member, within the team's `handoffs_per_day`. The lead's next run gets their results in `.muxa/TASK.md`; `.muxa/team-step` holds the step.

### Other agent output

Each of these is checked by the daemon, as Routine output is. Reasons go to `.muxa/CHECK.md`; the daemon does not type into these agents.

- **A team member** (`src/v2/team.rs`, ADR 0085) writes `.muxa/result.json`: `{"summary": "…", "memory"?: "…"}`. The summary is 1 byte to 8 KiB and goes to the lead as data; `memory` replaces the member's `memory.md`. A member that ends without one, or runs past an hour, gets no result.
- **Every Bot brief** (a run, a talk, a team member) opens with "You are a muxa Bot" from `src/routines/identity.rs` (ADR 0086): who owns the Soul and the Memory, that its Routines are muxa Routines, the Workspace's connections and MCP servers and whether claude.ai connectors load (a Bot has its Workspace's access and its `[workspace.bots]` rules only, and always runs Claude Code, ADR 0087), its own shell `commands` (ADR 0088; its runs and members use `--permission-mode dontAsk`, so any other command is refused; a Workspace with no `allow` gets the read defaults of ADR 0090), its daily item limit, what it may not change, and where it may propose settings (only in a talk).
- **A talk with a Bot** (`src/v2/talk.rs`, ADR 0083) starts Claude Code with `--append-system-prompt` telling it to read `.muxa/TASK.md` before it answers: the user speaks first, so a talk gets no start prompt as a run does, and without it the Bot never read its brief. It writes `.muxa/bot.json`: `{"memory"?: "…", "items"?: [Item…], "proposal"?: {"summary": "…", "ops": [Op…]}}`. `items` (ADR 0089) are up to 20 Inbox items the user asked for, in the Routine output's item format; they reach the Inbox at once and do not count against `daily_items`. Ops are `settings-update` ops limited to the Bot's own Bot and Routines in its Workspace. A passed proposal becomes one Decide item with `settings_proposal`.
- **The Capture assistant** (`src/v2/assistant.rs`, ADR 0084) writes `.muxa/suggestion.json`: `{"kind"?, "workspace"?, "due"?, "note"?, "split"?, "task_goal"?, "bot"?, "memory"?}`. It runs in `<state dir>/assistant/runs/<item>/`, and the folder goes after its turn.
- **A review Routine** (`on = "review"`, ADR 0084) writes Routine output as any Routine does. Its brief names the Task, whose places it may read and not edit.

## Agent adapters

Chat shows a coding agent's conversation as messages (`src/v2/chat.rs`). An **agent adapter** knows one agent: where it keeps its records and how to read them. Adapters live in `src/agents/`: Claude Code (`claude.rs`) and Codex (`codex.rs`). `log.rs` holds the bounded entry list they share.

```rust
trait AgentAdapter: Send + Sync {
    fn id(&self) -> &'static str;   // e.g. "claude-code"
    fn open_folder(&self, dir: &Path, procs: &Processes) -> Result<Option<Box<dyn ChatSource>>, Error>;
}

trait ChatSource: Send {
    fn poll(&mut self) -> Result<Vec<ChatEvent>, Error>; // first call: a Reset
    fn entries(&self) -> Vec<ChatEntry>;  // everything current
    fn status(&self) -> Option<Agent>;    // None when unknown
    fn prompt(&self) -> Option<ChatPrompt>; // what the agent waits on, with its keys
}
```

- **`open_folder`** returns a source for the adapter's agent working in a Task's working place, or `None`. The registry (`Registry::standard`) asks the adapters in order; the first source wins.
- **`poll`** runs about every 0.7 s on the blocking pool. It returns `Reset` (replace everything) or `Upsert` (add an entry, or replace the one with its id). `Error::Gone` ends the chat, for example when the agent exited.
- **`prompt`** names what the agent waits on and the keys that pick each choice (ADR 0009). The daemon turns the keys into the terminal input each option sends; an unknown key drops the whole prompt.
- **Data directories.** An adapter searches a list of directories (ADR 0016): every `--agent-dir <id>=PATH`, else its default, then each workspace's value of the adapter's variable (`CLAUDE_CONFIG_DIR`, `CODEX_HOME`) from `[workspace.agent]`. The daemon reads them when it starts.

Rules for an adapter:
1. **Find the agent by facts, not guesses.** Match its records to the folder and check that its process still runs. If nothing matches, return `None`.
2. **Skip what you don't understand.** Agent formats are private and change. An unknown line or field is dropped, never turned into a made-up entry.
3. **Report state only when the agent records it.**
4. **Never log content.** Log ids, sizes and errors.
5. **Bound everything.** Use `tail::JsonlTail` for append-only JSONL files: it starts from the last 1 MiB, skips lines over 8 MiB, and reports a rewritten file. Cap entry text and tool results.
6. **Values that become paths are validated**, for example a session id.
7. **Pin the format.** Check in a sanitized sample of the agent's files under `crates/muxa-bridge/tests/data/<agent>/`, with the agent version it came from, and test against it.

### Claude Code

- `CLAUDE_CONFIG_DIR` (default `~/.claude`) moves everything below, so the adapter searches each workspace's too.
- `~/.claude/sessions/<pid>.json`: one file per running instance, with `sessionId`, `cwd`, `kind`, `status` (`busy`, `waiting`, `idle`). The adapter takes the newest `kind: interactive` instance whose `cwd` is the folder and whose process still runs. Files of exited instances can remain.
- `~/.claude/projects/<cwd with non-alphanumerics as ->/<sessionId>.jsonl`: the conversation.
  - `user` lines hold prompts or `tool_result` blocks.
  - `assistant` lines hold one content block each (`text`, `tool_use`, `thinking`). Lines of one turn share `message.id` and are merged.
  - `isSidechain` (subagents) and `isMeta` lines are skipped, and so is thinking.
- `/clear` and `/resume` keep the process but change `sessionId`. The source then resets to the new transcript.
- After `/compact` or a rewind, the file is not linear. The chat may show messages that Claude Code no longer shows.
- While it waits (`status: waiting`), the last transcript line is the `tool_use` it waits on, with its full input. This was checked with 2.1.310 for a permission, AskUserQuestion and ExitPlanMode. The menu itself is not in the transcript. `claude/prompt.rs` offers the keys that held in every menu seen:
  - a permission: `1` allows, `Escape` denies and interrupts the turn;
  - one AskUserQuestion question with a single answer: the option's number picks it;
  - ExitPlanMode: `Escape` keeps planning. Which mode `1` approves with depends on settings, so the card offers only Keep planning (`docs/findings/claude-code-plan-menu.md`).

  With several open calls, several questions or several answers, no prompt is offered. Neither is one while a subagent (`Task`, `Agent`) runs: it asks about its own calls, which are sidechain lines.
- A folder Claude Code does not trust holds the agent on its trust dialog. Trusting the workspace root once trusts every folder in it (`docs/findings/claude-code-trust.md`). muxa never writes Claude Code's config.

### Codex

Checked with Codex CLI 0.160.0. Codex runs as a Task's agent and has Chat. It cannot run a Routine (above).

- `CODEX_HOME` (default `~/.codex`) moves `sessions/`.
- `~/.codex/sessions/YYYY/MM/DD/rollout-<time>-<id>.jsonl`: one conversation. The first line, `session_meta`, holds its `cwd`. `response_item` lines hold the messages and tool calls; `event_msg` lines `task_started` and `task_complete` give the state.
- Codex records no process id. The adapter takes the `codex` processes whose working directory is the folder: `ps` cannot show it, so it reads `/proc/<pid>/cwd` on Linux and asks `/usr/sbin/lsof` on macOS. With none, there is no Chat. The conversation is the newest rollout whose `cwd` is the folder, changed since the oldest such process started. Only the newest 7 day directories and the newest 64 rollouts are read. A Chat opened before the first prompt picks up the rollout when Codex writes it, and follows `/new`. The Chat ends when no `codex` runs in the folder. Not checked with a real Codex yet: that its process keeps the folder as its working directory.
- The state comes only from `task_started` (working) and `task_complete` or `turn_aborted` (idle).
- Codex does not record an approval prompt, so its Chat offers none.
