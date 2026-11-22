# 0069: An agent gets only the connections its Task or Routine names

## Status

Proposed (2026-11-21). Changes ADR 0016 (per-Workspace agent environments) and ADR 0026 (a Routine's sources are its agent's native tools).

Decided while building (2026-11-22): the move is gradual. A Workspace's `[workspace.agent] env` keeps going to every agent, so nothing that relies on it breaks. Connections are the way to take a token out of it: a connection reaches only the Tasks and Routines that name it, or all of them with `default = true`. Secrets are read with `/usr/bin/security`; no Keychain library is linked in.

## Context

ADR 0016 gives each Workspace an agent environment. Today muxa puts every filled token of the Workspace into every agent it starts. A personal API key there means every agent can act as the user in that service, and an agent that reads untrusted text (an issue, a web page) holds all of them.

The developer rejected a hosted assistant for the same reason: one place with full access to everything.

## Decision

1. **The goal is less ambient access, not a security boundary.** All agents run as the same user. An agent with a shell can still read other processes' environments, the server's socket and, with a prompt, the Keychain. This ADR removes access that agents get by default; it does not contain a hostile agent.
2. **A Workspace lists connections and MCP servers.** Each has a name, an account, an access level and, where muxa holds one, its secret.
3. **Secrets live in the macOS Keychain.** They are not kept in `workspaces.toml`, the Task folder, logs or the wire. The server reads them as a LaunchAgent in the user's GUI domain.
4. **A Task or Routine names the connections and MCP servers it uses.** muxa gives the agent only those:
   - environment variables for the connections it names;
   - for Claude Code, `--mcp-config` with only the named servers and `--strict-mcp-config`, so user-scope servers do not load;
   - for Codex, a config with only the named servers.
5. **Nothing is given by default.** A Workspace may name servers "on for every Task" (for example a docs server). Settings shows that list.
6. **Access levels are enforced by the token where the service allows it** (a read-only token, a scoped OAuth grant). Where it does not, the level is a label, and Settings says so.
7. **Settings show who uses what:** for each connection, the Tasks and Routines that name it, and when it was last used. An expired secret shows as such, and the Routines that need it fail with that reason.
8. **Routines run read only by default.** They write only to the Inbox. A Routine that must write elsewhere names a connection with write access, and the user sees it in Settings.

## Consequences

- An agent that reads untrusted text holds only the access its job needs.
- Moving to this breaks Tasks and Routines that relied on the shared environment. A migration step reads the current environment use and proposes the lists.
- A locked Keychain (after a reboot without login) makes an agent's start fail with a clear reason. A binary rebuilt by `make install` may ask for Keychain access again unless it is signed.
- `docs/privacy.md` and `SECURITY.md` must describe the new rule when this is built.

## Connectors of the agent's own login

claude.ai connectors and hosted OAuth MCP servers follow the Claude login in `CLAUDE_CONFIG_DIR`, not muxa. For now they stay with each Workspace's login (ADR 0016), and Settings lists them as "from the Claude login".

Later, muxa should hold several connections of one service at once, for example two Slack workspaces or two Linear workspaces, and give each Task or Routine the one it names (decided 2026-11-21 as the direction). That needs muxa to own the MCP config for those services.

## Addendum 2026-11-22: MCP servers (E2b)

Decided while building §2, §4 and §5 for MCP servers:

1. **Config.** `[[workspace.mcp]]` in `workspaces.toml`: a `name`, either `command` with `args` or `url` with `type` (`http` by default, or `sse`), plain `env` (command servers) or `headers` (url servers), `connections` and `default`. A Routine names servers with `mcp = [...]` in `routines.toml`; a v2 Task with `start-task {mcp}`. An unknown name is refused like an unknown connection.
2. **Secrets stay connections.** A server lists the connections it needs. The agent gets their variables in its environment, as for any connection. The config file refers to them as `${VAR}`, which Claude Code expands (`docs/findings/claude-code-mcp.md`). So a secret never lands in a file, and the Keychain stays the only place for it. Alternative: write the values into the file with mode 0600. Rejected: the file sits in the Task folder, which the Reader lists and the agent can copy.
3. **File.** `.muxa/mcp.json` in the working place, passed to Claude Code by absolute path with `--mcp-config`. `.muxa/` is already outside git and holds the other files muxa writes for the agent.
4. **Strict, also when empty.** Every Claude Code agent gets `--strict-mcp-config`, also with no servers (`{"mcpServers":{}}`). §5 says nothing is given by default, and user-scope servers from `CLAUDE_CONFIG_DIR` would otherwise reach every agent. This is a change for setups that relied on those servers: they must become `[[workspace.mcp]]` entries. Whether claude.ai connectors also stop is not verified; they stay with the login as decided above.
5. **Codex gets none yet.** Codex documents `[mcp_servers.<name>]` in `config.toml`, but no confirmed per-run override and no way to skip the user's own servers, so §4's "only the named servers" cannot hold. A Task or Routine run whose agent is not Claude Code, and that would get a server (named or on by default), is refused before anything is made, with a reason that says so. Without servers, Codex starts as before.

## Not decided

1. Where secrets live on a Linux host without a desktop secret store.
2. How muxa holds several OAuth connections of one service, and when it takes over MCP config from the agent's login.
