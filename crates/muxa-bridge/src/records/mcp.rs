//! A workspace's `[[workspace.mcp]]` tables (ADR 0069): MCP servers an
//! agent gets only when its Task or Routine names them, or when they are on
//! by default. Nothing here is a secret: a server's secrets come from the
//! connections it names, as `${VAR}` references.

use std::collections::BTreeMap;

use serde::Deserialize;

use super::connections::{Connection, is_env_key, is_name};

const MAX_VALUE: usize = 4096;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Transport {
    Stdio { command: String, args: Vec<String> },
    Http { url: String },
    Sse { url: String },
}

/// An MCP server as configured.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpServer {
    pub name: String,
    pub transport: Transport,
    /// Plain variables of a command server; never secrets.
    pub env: BTreeMap<String, String>,
    /// Headers of a url server; `${VAR}` names a variable of `connections`.
    pub headers: BTreeMap<String, String>,
    /// The workspace's connections whose variables the server needs.
    pub connections: Vec<String>,
    /// Given to every Task and Routine of the workspace.
    pub default: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RawMcp {
    name: String,
    command: Option<String>,
    args: Option<Vec<String>>,
    url: Option<String>,
    #[serde(rename = "type")]
    kind: Option<String>,
    #[serde(default)]
    env: BTreeMap<String, String>,
    #[serde(default)]
    headers: BTreeMap<String, String>,
    #[serde(default)]
    connections: Vec<String>,
    #[serde(default)]
    default: bool,
}

impl RawMcp {
    fn check(self, known: &[Connection]) -> Result<McpServer, String> {
        if !is_name(&self.name) {
            return Err(format!(
                "mcp name {:?} must be 1 to 64 of a-z, 0-9 and -",
                self.name
            ));
        }
        let named = |r: &str| format!("mcp {:?}: {r}", self.name);
        let transport = self.transport().map_err(|r| named(&r))?;
        let mut vars = Vec::with_capacity(self.connections.len());
        for (i, c) in self.connections.iter().enumerate() {
            let Some(found) = known.iter().find(|k| &k.name == c) else {
                return Err(named(&format!(
                    "connection {c:?} is not a [[workspace.connection]] of this workspace"
                )));
            };
            if self.connections[..i].contains(c) {
                return Err(named(&format!("connection {c:?} appears twice")));
            }
            vars.push(found.env.as_str());
        }
        let stdio = matches!(transport, Transport::Stdio { .. });
        if !stdio && !self.env.is_empty() {
            return Err(named(
                "env is for a command server; a url server takes headers",
            ));
        }
        if stdio && !self.headers.is_empty() {
            return Err(named(
                "headers are for a url server; a command server takes env",
            ));
        }
        for (key, value) in &self.env {
            if !is_env_key(key) {
                return Err(named(&format!(
                    "env {key:?} must be an environment variable name"
                )));
            }
            if vars.contains(&key.as_str()) {
                return Err(named(&format!(
                    "env sets {key}, which a connection of this server gives"
                )));
            }
            plain(value).map_err(|r| named(&format!("env {key}: {r}")))?;
        }
        for (key, value) in &self.headers {
            if key.is_empty() || !key.bytes().all(is_header_byte) {
                return Err(named(&format!("header {key:?} is not a header name")));
            }
            plain(value).map_err(|r| named(&format!("header {key}: {r}")))?;
            if let Some(other) = references(value).find(|r| !vars.contains(r)) {
                return Err(named(&format!(
                    "header {key} uses ${{{other}}}, which none of this server's connections gives"
                )));
            }
        }
        Ok(McpServer {
            name: self.name,
            transport,
            env: self.env,
            headers: self.headers,
            connections: self.connections,
            default: self.default,
        })
    }

    fn transport(&self) -> Result<Transport, String> {
        match (&self.command, &self.url) {
            (Some(command), None) => {
                if !matches!(self.kind.as_deref(), None | Some("stdio")) {
                    return Err("a command server's type is \"stdio\"".into());
                }
                if command.trim().is_empty() {
                    return Err("command is empty".into());
                }
                plain(command).map_err(|r| format!("command: {r}"))?;
                let args = self.args.clone().unwrap_or_default();
                for (i, a) in args.iter().enumerate() {
                    plain(a).map_err(|r| format!("args[{i}]: {r}"))?;
                }
                Ok(Transport::Stdio {
                    command: command.clone(),
                    args,
                })
            }
            (None, Some(url)) => {
                if self.args.is_some() {
                    return Err("args are for a command server".into());
                }
                let web = url.starts_with("https://") || url.starts_with("http://");
                if !web
                    || url.len() > MAX_VALUE
                    || url.chars().any(|c| c.is_whitespace() || c.is_control())
                {
                    return Err("url must be one http:// or https:// address".into());
                }
                match self.kind.as_deref() {
                    None | Some("http") => Ok(Transport::Http { url: url.clone() }),
                    Some("sse") => Ok(Transport::Sse { url: url.clone() }),
                    Some(_) => Err("a url server's type is \"http\" or \"sse\"".into()),
                }
            }
            _ => Err("give either command (with args) or url".into()),
        }
    }
}

/// Checks a workspace's list against its connections: each name once.
pub(super) fn check_all(raw: Vec<RawMcp>, known: &[Connection]) -> Result<Vec<McpServer>, String> {
    let mut out: Vec<McpServer> = Vec::with_capacity(raw.len());
    for r in raw {
        let server = r.check(known)?;
        if out.iter().any(|o| o.name == server.name) {
            return Err(format!("mcp {:?} appears twice", server.name));
        }
        out.push(server);
    }
    Ok(out)
}

/// The names in `${NAME}` references of `value`.
pub fn references(value: &str) -> impl Iterator<Item = &str> {
    value
        .split("${")
        .skip(1)
        .filter_map(|rest| rest.split_once('}').map(|(name, _)| name))
}

/// One line of at most [`MAX_VALUE`] bytes.
fn plain(value: &str) -> Result<(), String> {
    if value.len() > MAX_VALUE || value.chars().any(char::is_control) {
        return Err(format!("must be one line of at most {MAX_VALUE} bytes"));
    }
    Ok(())
}

/// RFC 9110 token characters.
fn is_header_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&b)
}
