# Claude Code's MCP config flags: what muxa relies on

muxa gives a Claude Code agent only the MCP servers its Task or Routine names (ADR 0069, `crates/muxa-bridge/src/v2/mcp.rs`). These facts come from Claude Code's documentation (`code.claude.com/docs/en/mcp` and `/cli-reference`), read 2026-11-22. Rows marked *docs* were not probed against a binary; no repro script checks them yet, because a probe needs a Claude Code login. Re-check them after a Claude Code upgrade.

| Fact | Source |
|---|---|
| `--mcp-config <file>` loads MCP servers from a JSON file of the shape `{"mcpServers":{"<name>":{...}}}`. | *docs* |
| `--strict-mcp-config` skips the servers configured in places such as `.mcp.json`, `~/.claude.json` or a plugin, and loads only the ones from `--mcp-config`. | *docs* |
| A server entry is `{"type":"stdio","command","args","env"}`, or `{"type":"http"\|"sse","url","headers"}`. An entry with a `url` and no `type` is an error. | *docs* |
| `${VAR}` and `${VAR:-default}` expand in a server's `command`, `args`, `env`, `url` and `headers`. The docs describe this for `.mcp.json`; muxa assumes the same for a `--mcp-config` file. | *docs*, assumption for `--mcp-config` |
| An unset `${VAR}` with no default is kept as the literal text, and the config still loads. | *docs* |
| In a remote server's `url` and `headers`, Claude Code's own and the cloud provider's credentials (`ANTHROPIC_API_KEY`, `ANTHROPIC_AUTH_TOKEN`, `AWS_BEARER_TOKEN_BEDROCK`, `NPM_TOKEN`, `HTTPS_PROXY`, …) read as empty. Other names, such as `LINEAR_API_KEY`, expand. | *docs* |
| The docs do not say whether claude.ai connectors still load under `--strict-mcp-config`. `ENABLE_CLAUDEAI_MCP_SERVERS=false` turns them off; servers from `--mcp-config` are not affected by it. | *docs* (open) |

## What muxa does with it

- The file is `.muxa/mcp.json` in the working place, passed by absolute path. A connection's variable appears only as `${VAR}`; the value goes into the agent's environment.
- In a workspace that lists any `[[workspace.mcp]]`, every Claude Code agent gets `--strict-mcp-config`, also with no servers chosen (`{"mcpServers":{}}`), so the servers of the user's own config do not load (ADR 0069, §5). A workspace without any gets no MCP flags.
- A connection whose variable is one of the names that read as empty cannot authenticate a remote server through `headers`. Give it a name of its own.
- Codex: its docs describe `[mcp_servers.<name>]` tables in `config.toml` and an `enabled` key, but no per-run override and no way to skip the user's servers. muxa gives Codex no MCP servers and refuses a start that would.
