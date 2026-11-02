# 0016: Per-workspace agent environment

## Status

Proposed.

## Context

- A user's workspaces use different accounts. The company and the personal projects have different Slack, Linear and Notion accounts, and different MCP servers.
- Today every Task starts the same agent command (`--task-command`) with the tmux server's environment. One Claude Code login, one set of claude.ai connectors and one set of user-scope MCP servers serve every workspace.
- The vision says each Workspace has its own credentials, and an agent in one Workspace does not get another's (`docs/vision.md`).
- The agents already separate accounts by a directory, chosen with an environment variable. What each variable separates was checked against the vendors' docs on 2026-11-02:

  | Variable | Separates (documented) | Source |
  |---|---|---|
  | `CLAUDE_CONFIG_DIR` (default `~/.claude`) | Settings, session history, plugins. The claude.ai login: `.credentials.json` moves there, and the macOS Keychain entry is keyed to the directory. `.claude.json` moves there too, and with it user-scope and local-scope MCP servers and the account details. The docs name it for "running multiple accounts side by side". | code.claude.com: env-vars, authentication ("Log in with multiple accounts", "Credential management"), claude-directory, mcp |
  | claude.ai connectors | Fetched for the claude.ai account that is logged in. So they follow the login, and with it `CLAUDE_CONFIG_DIR`. They are not loaded when `ANTHROPIC_API_KEY`, `ANTHROPIC_AUTH_TOKEN` or a `setup-token` token is active. | code.claude.com: mcp ("Use MCP servers from claude.ai") |
  | Not separated by `CLAUDE_CONFIG_DIR` | A Claude Console sign-in without an API key (an Anthropic profile in `~/.config/anthropic`). Project-scope MCP (`.mcp.json` in the repo). Managed settings. | code.claude.com: authentication |
  | `CODEX_HOME` (default `~/.codex`) | `config.toml`, so user-level `[mcp_servers]`. `auth.json` when credentials are file-based. `history.jsonl`, logs and caches. | learn.chatgpt.com: config-advanced, mcp |
  | Not documented for `CODEX_HOME` | Whether `sessions/` moves (third-party sources say all state does). Whether a keyring login is keyed per `CODEX_HOME`. | |

- A probe on a throwaway `-L` server with Claude Code 2.1.316 and an empty `CLAUDE_CONFIG_DIR`: Claude Code created `.claude.json`, `sessions/`, `backups/` and `cache/` in that directory and showed first-run setup. It wrote nothing to `~/.claude/sessions/`. Only two real logins can prove that accounts, connectors and MCP servers stay apart; see Consequences.
- The Claude Code adapter finds its agent through `<config dir>/sessions/<pid>.json` (ADR 0005), and Codex through `<CODEX_HOME>/sessions/`. With a per-workspace directory, chat, states (ADR 0008) and approval cards (ADR 0009) would silently stop for those panes.

## Decision

### Config

A workspace gets an optional `[workspace.agent]` table in `workspaces.toml`:

```toml
[[workspace]]
name = "Company"
root = "~/work/company"
repos = ["~/work/company/api"]

[workspace.agent]
command = "claude"     # optional; else --task-command
env = { CLAUDE_CONFIG_DIR = "~/.claude-company", CODEX_HOME = "~/.codex-company" }
```

- Unknown keys are still refused (`deny_unknown_fields`), in the table too.
- `command` is optional, not empty, and has no control character. It runs through the shell, like `--task-command`.
- An env key is a portable name, `[A-Za-z_][A-Za-z0-9_]*`. Any such key is allowed, except `TMUX` and `TMUX_PANE`.
  - Not an allowlist. The file is host config, written by the host's owner, like the `--task-command` that already runs any program. An allowlist would block real needs, such as `GH_CONFIG_DIR` for a second GitHub account, or a token an MCP server reads, and give no protection the file's owner could not undo.
  - `TMUX` and `TMUX_PANE` are refused because tmux sets them, and Claude Code records the pane from them. A wrong value would break the match of ADR 0005.
- A value has no control character and is at most 4096 bytes. At most 64 keys.
- A value that starts with `~/` gets the home directory, as a shell does after `NAME=`. Nothing else is expanded: no `$VAR`, no `~user`.
- `CLAUDE_CONFIG_DIR` and `CODEX_HOME` must be absolute after expansion, because the adapters search them. They need not exist yet: the agent creates them on first run.
- Error messages name the workspace and the key, never a value.

### Values are secrets

- Env values may hold tokens. The bridge logs keys only, never values (invariant 5 now covers them).
- The config types redact values in `Debug`.
- The built-in launcher sends `new-session -e` on the control client's stdin, not as a process argument, so no other local user sees the values in `ps`.
- tmux keeps `-e` values in the session environment. `tmux show-environment -t <session>` shows them to the same user. That is no new exposure: the same user can read `workspaces.toml`.

### How the env reaches the agent

**The built-in launcher** (repo Tasks without tmux-agents, and every repo-less Task) adds `-e KEY=VALUE` to `new-session`, one per key, through the quoting function of `muxa-tmux` (invariant 4). The agent command comes from `command`, else `--task-command`.

- tmux added `-e` to `new-session` in 3.2 (CHANGES, "3.1c to 3.2"). CI's oldest tmux is 3.2a.
- `scripts/repro/session-env.sh` checks what the design relies on: the pane process sees each value byte for byte; a value overrides the same key in the global environment; the global environment does not change; a new window in that session gets the values too.

**tmux-agents.** `new.sh` makes the session itself, so the bridge cannot add `-e`. ADR 0017 (in progress) owns the tmux-agents side. The contract this ADR needs:

1. The bridge runs `new.sh` with each value in `new.sh`'s own environment, and the list of keys in `TMUX_AGENTS_ENV` (space-separated). Values never go on `new.sh`'s command line, where `ps` shows them.
2. `new.sh` makes every listed variable reach the agent pane's process, and any later pane of that session.
3. The agent command comes from `TMUX_AGENTS_COMMAND` when set.
4. tmux-agents says it supports this in a way the bridge can read before it runs `new.sh`, for example a global option.

Until that exists, a Task whose workspace has an `agent` table always uses the built-in launcher, also when tmux-agents is installed or forced with `--task-launcher`. The task then has no tmux-agents layout. A wrong account is worse than a missing layout.

### Which workspace a Task belongs to

- `create-task` names its workspace. Its `agent` table applies, for a repo Task and for a repo-less Task.
- A repo Task's repo is matched by its main checkout against every workspace's `repos`. When another workspace lists it and this one does not, `create-task` refuses: the agent would run in a repo of another workspace with this workspace's accounts.
- `new-task` names no workspace. The workspace whose `repos` lists the repo's main checkout applies. When no workspace or more than one lists it, the agent starts as before, with no workspace environment.
- When the branch already has a session, the launcher returns it as before. Its agent keeps the environment it was started with.

### Adapters follow the env

- Each adapter searches a list of data directories, not one:
  1. every `--agent-dir ID=PATH`, which may now repeat; else the default (`~/.claude`, `~/.codex`);
  2. then every workspace's `CLAUDE_CONFIG_DIR` (Claude Code) or `CODEX_HOME` (Codex), without duplicates.
- The matching rules of ADR 0005 stay. For Claude Code, a session file matches only when the pane id **and** the session name match, and its pid runs inside the pane. Files from all directories are candidates together, and the same order picks one. The chosen file's directory is then used for the transcript and the plan.
- For Codex, the newest rollout with the pane's cwd since the process started wins, across all directories.
- No state is invented (invariant 6). A pane whose agent writes to a directory no list names stays `unsupported`, as today.
- The bridge reads `workspaces.toml` once at start for the adapter directories. A bridge runs per SSH connection, so the next connection sees an edit. Task launches read the file on every request, as before.

### Manual Mode

Nothing changes. A user who runs an agent with another config directory in a pane of their own, for example `alias claude-work='CLAUDE_CONFIG_DIR=~/.claude-work claude'`, has two ways to let the adapters see it:

- list that directory in some workspace's `agent.env`;
- or pass `--agent-dir claude-code=~/.claude --agent-dir claude-code=~/.claude-work` in the SSH forced command.

## Consequences

- One workspace config switches accounts for Claude Code and Codex Tasks, and the adapters keep working for them.
- What the user must check by hand, with two real logins, because no test can log in:
  1. Start a Task in each workspace. `/status` in each agent shows the expected account.
  2. `/mcp` in each lists only that account's claude.ai connectors and that config's user-scope MCP servers.
  3. Logging out in one does not log out the other. On macOS, both logins survive a restart (two Keychain entries).
  4. For Codex: `codex login status` differs per `CODEX_HOME`, and new rollouts appear under each `CODEX_HOME/sessions/`.
  5. The iPad shows chat, state and approval cards for both agents.
- Until ADR 0017 lands, workspaces with an `agent` table lose the tmux-agents layout for their repo Tasks.
- `--agent-dir` may now repeat for one agent. One flag still replaces the default, as before.
- Rejected: an allowlist of env keys. See Config.
- Rejected: per-workspace `--agent-dir` flags. Two sources of truth for one directory would drift; the agent's own variable is the one that decides where it writes.
- Rejected: `env -i` or a cleared environment for the agent. The agent needs PATH, the locale and the user's shell setup; the workspace adds to the environment, it does not replace it.
- Rejected: passing env values to tmux-agents as `new.sh` arguments. Process arguments are visible in `ps`.
