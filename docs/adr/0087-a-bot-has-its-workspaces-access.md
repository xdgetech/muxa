# 0087: A Bot has its Workspace's access, and nothing else

## Status

Proposed (2026-11-23). Amends ADR 0082 (a Bot's `worker`) and ADR 0075 (`--strict-mcp-config`). Wire: `protocol/PROTOCOL-v2.md`, `BotSpec`, `SettingsWorkspace.claude_connectors`, `workspace-update`.

## Context

A Bot could name a worker, and a worker carries its own MCP config and permission settings. In practice the developer's workers had become roles (chief of staff, strategist), so a Bot `chief-of-staff` on the worker `lisk-chief-of-staff` said the same thing twice, and choosing a worker per Bot was configuration nobody wanted. The developer's first request was that a Bot's permissions follow its Workspace. ADR 0075 already gives every agent of a Workspace its connections and MCP servers.

## Decision

1. **A Bot runs with its Workspace's access.** Its Routine runs, its talks and its team members run Claude Code with the Workspace's env, connections and MCP servers. The command is the Workspace's agent when that is Claude Code (its flags and model stay), else `claude`, so a Workspace whose Tasks run Codex can still have Bots. A Bot always gets the Workspace's MCP config with `--strict-mcp-config`, also when the Workspace lists no server, so the user's own servers and claude.ai connectors never reach it unless decision 4 lets them in. A `worker` on a `[[bot]]` or on a Bot's `[[routine]]` is read and ignored; clients stop offering it. Routines without a Bot and Tasks keep workers.
2. **One set of rules for a Workspace's Bots.** `[workspace.bots]` holds `allow` and `deny` lists of Claude Code permission rules. Claude Code enforces them on every Bot of the Workspace, beside muxa's own deny rules (`.muxa/`, read-only repos). Tasks never get them, so a Workspace can deny Bots the shell while its coding Tasks keep it. Each Bot is told its rules.
3. **The Workspace is the only boundary.** To give a Bot a tool, add it to the Workspace. To keep data from a Bot, put it in another Workspace.
4. **A Workspace may let claude.ai connectors in.** With `claude_connectors = true`, agents of a Workspace with MCP servers get `--mcp-config` without `--strict-mcp-config`, so the user's claude.ai connectors (Calendar, Sheets) load beside the Workspace's servers. Off by default. The Capture assistant stays strict.
5. **A Bot is told its access.** The muxa Bot section (ADR 0086) lists the Workspace's connections and MCP servers and says whether claude.ai connectors load.

## Consequences

- One thing to configure per Workspace instead of one per Bot: its tools, and its Bots' rules.
- A Workspace's Bots all see the same data. A role that needs a private source (the team assessment) goes in its own Workspace.
- A user whose tools sit in a worker's `--mcp-config` moves them to the Workspace's MCP servers, and its secrets to connections in the Keychain.
- Alternatives not taken:
  - Access settings on each Bot: a second place for MCP and permission settings, in the file a Bot's talk proposes changes to.
  - Workers renamed as access profiles: it keeps a second concept the user must learn for no gain in a Workspace with one set of tools.
