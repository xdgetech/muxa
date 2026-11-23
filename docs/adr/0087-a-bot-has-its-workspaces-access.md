# 0087: A Bot has its Workspace's access, and nothing else

## Status

Proposed (2026-11-23). Amends ADR 0082 (a Bot's `worker`) and ADR 0075 (`--strict-mcp-config`). Wire: `protocol/PROTOCOL-v2.md`, `BotSpec`, `SettingsWorkspace.claude_connectors`, `workspace-update`.

## Context

A Bot could name a worker, and a worker carries its own MCP config and permission settings. In practice the developer's workers had become roles (chief of staff, strategist), so a Bot `chief-of-staff` on the worker `lisk-chief-of-staff` said the same thing twice, and choosing a worker per Bot was configuration nobody wanted. The developer's first request was that a Bot's permissions follow its Workspace. ADR 0075 already gives every agent of a Workspace its connections and MCP servers.

## Decision

1. **A Bot runs with its Workspace's access.** Its Routine runs, its talks and its team members run as the Workspace's agent, with the Workspace's connections and MCP servers. A `worker` on a `[[bot]]` or on a Bot's `[[routine]]` is read and ignored; clients stop offering it. Routines without a Bot and Tasks keep workers.
2. **The Workspace is the only boundary.** To give a Bot a tool, add it to the Workspace. To keep data from a Bot, put it in another Workspace.
3. **A Workspace may let claude.ai connectors in.** With `claude_connectors = true`, agents of a Workspace with MCP servers get `--mcp-config` without `--strict-mcp-config`, so the user's claude.ai connectors (Calendar, Sheets) load beside the Workspace's servers. Off by default. The Capture assistant stays strict.
4. **A Bot is told its access.** The muxa Bot section (ADR 0086) lists the Workspace's connections and MCP servers and says whether claude.ai connectors load.

## Consequences

- One thing to configure per Workspace instead of one per Bot.
- A Workspace's Bots all see the same data. A role that needs a private source (the team assessment) goes in its own Workspace.
- A user whose tools sit in a worker's `--mcp-config` moves them to the Workspace's MCP servers, and its secrets to connections in the Keychain.
- Alternatives not taken:
  - Access settings on each Bot: a second place for MCP and permission settings, in the file a Bot's talk proposes changes to.
  - Workers renamed as access profiles: it keeps a second concept the user must learn for no gain in a Workspace with one set of tools.
