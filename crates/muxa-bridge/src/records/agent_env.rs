//! A workspace's `[workspace.agent]` table: the command and environment its
//! agents start with (ADR 0016). Values may be tokens: they never reach a log
//! or an error message, and `Debug` redacts them.

use std::collections::BTreeMap;
use std::fmt;
use std::path::{Path, PathBuf};

use serde::Deserialize;

/// Bytes, per value.
const MAX_VALUE: usize = 4096;
const MAX_KEYS: usize = 64;
/// tmux sets them, and Claude Code finds its pane from them (ADR 0005).
const RESERVED: [&str; 2] = ["TMUX", "TMUX_PANE"];
/// Agents read these as their data directory; the adapters search them, so
/// they must not depend on a working directory.
pub const PATH_KEYS: [&str; 2] = ["CLAUDE_CONFIG_DIR", "CODEX_HOME"];

/// How a workspace's agents start. Not empty: a workspace without one has None.
#[derive(Clone, PartialEq, Eq)]
pub struct AgentSettings {
    /// Through the shell, like `--task-command`; None means the flag's.
    pub command: Option<String>,
    /// Sorted by key; `~/` already expanded.
    pub env: Vec<(String, String)>,
}

impl AgentSettings {
    pub fn env(&self, key: &str) -> Option<&str> {
        self.env
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
    }

    /// The keys, for a log line.
    pub fn keys(&self) -> Vec<&str> {
        self.env.iter().map(|(k, _)| k.as_str()).collect()
    }
}

impl fmt::Debug for AgentSettings {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AgentSettings")
            .field("command", &self.command)
            .field("env", &self.keys())
            .finish()
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RawAgent {
    command: Option<String>,
    #[serde(default)]
    env: BTreeMap<String, String>,
}

impl RawAgent {
    /// Checks the table; `home` expands a leading `~/` in values. The error
    /// names the key, never the value.
    pub(super) fn check(self, home: Option<&Path>) -> Result<Option<AgentSettings>, String> {
        if let Some(c) = &self.command {
            if c.trim().is_empty() {
                return Err("agent command is empty".into());
            }
            if c.chars().any(char::is_control) {
                return Err("agent command has a control character".into());
            }
        }
        if self.env.len() > MAX_KEYS {
            return Err(format!("agent env has more than {MAX_KEYS} keys"));
        }
        let env = self
            .env
            .into_iter()
            .map(|(k, v)| {
                let v = value(&k, &v, home)?;
                Ok((k, v))
            })
            .collect::<Result<Vec<_>, String>>()?;
        if self.command.is_none() && env.is_empty() {
            return Ok(None);
        }
        Ok(Some(AgentSettings {
            command: self.command,
            env,
        }))
    }
}

fn value(key: &str, raw: &str, home: Option<&Path>) -> Result<String, String> {
    if !valid_key(key) {
        return Err(format!(
            "agent env key {key:?} is not a variable name: use letters, digits and _"
        ));
    }
    if RESERVED.contains(&key) {
        return Err(format!("agent env key {key} is set by tmux; remove it"));
    }
    if raw.len() > MAX_VALUE {
        return Err(format!("agent env {key} is longer than {MAX_VALUE} bytes"));
    }
    if raw.chars().any(char::is_control) {
        return Err(format!("agent env {key} has a control character"));
    }
    let expanded = match raw.strip_prefix("~/") {
        Some(rest) => home
            .ok_or_else(|| format!("agent env {key} starts with ~/ but HOME is not set"))?
            .join(rest)
            .to_string_lossy()
            .into_owned(),
        None => raw.to_owned(),
    };
    if PATH_KEYS.contains(&key) && !Path::new(&expanded).is_absolute() {
        return Err(format!(
            "agent env {key} must be an absolute path or start with ~/"
        ));
    }
    Ok(expanded)
}

/// `[A-Za-z_][A-Za-z0-9_]*`, the portable shell variable name.
pub fn valid_key(key: &str) -> bool {
    let mut chars = key.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// The absolute data directories that `key` names across `settings`, in order, without duplicates.
pub fn dirs_of<'a>(
    settings: impl IntoIterator<Item = &'a AgentSettings>,
    key: &str,
) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = Vec::new();
    for dir in settings
        .into_iter()
        .filter_map(|s| s.env(key))
        .map(PathBuf::from)
    {
        if dir.is_absolute() && !out.contains(&dir) {
            out.push(dir);
        }
    }
    out
}
