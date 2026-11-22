//! What an agent gets of its workspace's MCP servers (ADR 0069): the ones on
//! by default and the ones its Task or Routine names. Once a workspace lists
//! any `[[workspace.mcp]]`, Claude Code gets them in `.muxa/mcp.json` with
//! `--strict-mcp-config`, so no other configured server loads. The file holds
//! `${VAR}` references, never a secret: the values reach the agent's
//! environment as connections.

use std::path::{Path, PathBuf};

use serde_json::{Map, Value, json};

use super::git;
use crate::records::Workspace;
use crate::records::connections::Connection;
use crate::records::mcp::{McpServer, Transport};
use crate::records::workers::LaunchCommand;

const FILE: &str = "mcp.json";
const CONFIG_FLAG: &str = "--mcp-config";
const STRICT_FLAG: &str = "--strict-mcp-config";

/// muxa owns the agent's MCP config only in a workspace that lists servers.
/// Without any, the agent keeps the servers and connectors of its own login,
/// so a workspace set up before E2b works as before.
pub fn managed(ws: &Workspace) -> bool {
    !ws.mcp.is_empty()
}

/// The servers an agent gets: the default ones and `named`, in the
/// workspace's order. An unknown name is an error.
pub fn chosen(ws: &Workspace, named: &[String]) -> Result<Vec<McpServer>, String> {
    if let Some(unknown) = named.iter().find(|n| !ws.mcp.iter().any(|m| &m.name == *n)) {
        return Err(format!(
            "Workspace {} has no MCP server {unknown:?}; add it as [[workspace.mcp]] in workspaces.toml",
            ws.name
        ));
    }
    Ok(ws
        .mcp
        .iter()
        .filter(|m| m.default || named.contains(&m.name))
        .cloned()
        .collect())
}

/// `named` and the connections `servers` need, once each.
pub fn with_connections(named: &[String], servers: &[McpServer]) -> Vec<String> {
    let mut out = named.to_vec();
    for c in servers.iter().flat_map(|s| &s.connections) {
        if !out.contains(c) {
            out.push(c.clone());
        }
    }
    out
}

/// Only Claude Code takes muxa's MCP config. `program` is the agent's.
pub fn only_claude(program: Option<&str>, servers: &[McpServer]) -> Result<(), String> {
    if servers.is_empty() {
        return Ok(());
    }
    let names: Vec<&str> = servers.iter().map(|s| s.name.as_str()).collect();
    Err(format!(
        "MCP servers ({}) need Claude Code, and this agent is {}; muxa gives no MCP servers to Codex or other agents yet",
        names.join(", "),
        program.unwrap_or("not Claude Code")
    ))
}

/// Pure. The `--mcp-config` file: only `servers`, with each connection's
/// variable as a `${VAR}` reference that Claude Code expands.
pub fn config_text(servers: &[McpServer], known: &[Connection]) -> String {
    let mut all = Map::new();
    for s in servers {
        let vars = s
            .connections
            .iter()
            .filter_map(|c| known.iter().find(|k| &k.name == c))
            .map(|k| k.env.as_str());
        let mut entry = match &s.transport {
            Transport::Stdio { command, args } => {
                let mut env: Map<String, Value> = s
                    .env
                    .iter()
                    .map(|(k, v)| (k.clone(), Value::from(v.as_str())))
                    .collect();
                for var in vars {
                    env.insert(var.to_owned(), Value::from(format!("${{{var}}}")));
                }
                json!({"type": "stdio", "command": command, "args": args, "env": env})
            }
            Transport::Http { url } => json!({"type": "http", "url": url}),
            Transport::Sse { url } => json!({"type": "sse", "url": url}),
        };
        if !s.headers.is_empty() {
            entry["headers"] = json!(s.headers);
        }
        all.insert(s.name.clone(), entry);
    }
    let mut text = serde_json::to_string_pretty(&json!({ "mcpServers": all })).unwrap_or_default();
    text.push('\n');
    text
}

/// Writes `.muxa/mcp.json` in `dir`, even with no servers: an empty file
/// with the strict flag keeps user and project servers out. Blocking.
pub fn write(dir: &Path, servers: &[McpServer], known: &[Connection]) -> Result<PathBuf, String> {
    git::write_file(dir, FILE, &config_text(servers, known))
}

/// The flags for a shell line.
pub fn shell_flags(config: &Path) -> String {
    let path = config.to_string_lossy();
    format!(
        "{CONFIG_FLAG} '{}' {STRICT_FLAG}",
        path.replace('\'', "'\\''")
    )
}

/// `command` with the flags, before any prompt the caller adds.
pub fn with_flags(command: LaunchCommand, config: &Path) -> LaunchCommand {
    match command {
        LaunchCommand::Shell(line) => {
            LaunchCommand::Shell(format!("{line} {}", shell_flags(config)))
        }
        LaunchCommand::Native { program, mut args } => {
            args.extend([
                CONFIG_FLAG.to_owned(),
                config.to_string_lossy().into_owned(),
                STRICT_FLAG.to_owned(),
            ]);
            LaunchCommand::Native { program, args }
        }
    }
}

#[cfg(test)]
#[path = "mcp_tests.rs"]
mod tests;
