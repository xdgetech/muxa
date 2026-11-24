# 0090: Safe default rules for a Workspace's Bots

Status: accepted

## Context

A Bot's Routine runs and team members use `--permission-mode dontAsk` (ADR 0088), so any tool without an allow rule is refused. ADR 0087 removed the per-worker settings files that held those rules. A Workspace with no `[workspace.bots]` gave its Bots no web and no connector tools, so their runs found nothing. The developer wants safe tools allowed by default and changed later in Settings.

## Decision

1. **A Workspace whose `[workspace.bots]` has no `allow` gets `DEFAULT_ALLOW`** (`crates/muxa-bridge/src/records/bots.rs`). It holds only tools that read and never write:
   - `WebSearch` and `WebFetch`;
   - the read tools of the common claude.ai connectors (Calendar, Drive, Docs, Sheets, Slack, Notion, Linear, Meetings), by name or by a prefix wildcard such as `mcp__claude_ai_Linear__get_*`. They matter only when the Workspace sets `claude_connectors`.
2. **Gmail stays out.** Mail is where untrusted text reaches the user. A Bot that reads it and also fetches the web is the easiest leak to set up, so the user adds Gmail to the rules on purpose.
3. **Settings shows the defaults as the Workspace's rules**, so the user edits them like any other rule. Saving the defaults unchanged leaves `allow` out of the file, so the Workspace follows muxa's defaults when they change. An empty list is written as `allow = []` and allows nothing.
4. muxa's own allow rules from ADR 0088 (the run's folder, reads in the Workspace, the Workspace's MCP servers) stay on top of these. Deny rules still win over every allow rule.

## Consequences

- New Workspaces' Bots can search the web and read the connectors the user turned on, without setup.
- MCP servers that load from a repo's `.mcp.json` or the user's own config, not from `[[workspace.mcp]]`, still need rules. Listing them in `[[workspace.mcp]]` makes them the Workspace's, so they are allowed.
- A claude.ai connector tool that is renamed falls out of the defaults silently. The Bot then reports it as refused.
- A Workspace that wrote its own `allow` keeps exactly that list.
