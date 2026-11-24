//! `[assistant]` in `workspaces.toml` (ADR 0084): the Capture assistant,
//! which belongs to no Workspace. Its Soul and Memory live in `assistant/`
//! beside `workspaces.toml`.
use std::path::Path;

use super::bots::SoulFiles;
use super::subscriptions::{self, Subscription};
use super::workers::{self, Worker, WorkerAgent};

pub const DIR: &str = "assistant";

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Assistant {
    pub enabled: bool,
    /// None: the default agent.
    pub worker: Option<String>,
    /// The Claude logins it runs on, in order (ADR 0091). Empty: the
    /// default login.
    pub subscriptions: Vec<String>,
}

/// Reads `[assistant]`. Keys of the removed assistant layer (ADR 0066) are
/// ignored, so an old file still loads.
pub(super) fn parse(
    value: Option<&toml::Value>,
    workers: &[Worker],
    all: &[Subscription],
) -> Result<Assistant, String> {
    let Some(table) = value.and_then(toml::Value::as_table) else {
        return Ok(Assistant::default());
    };
    let enabled = match table.get("enabled") {
        None => false,
        Some(v) => v
            .as_bool()
            .ok_or("[assistant] enabled must be true or false")?,
    };
    let worker = match table.get("worker") {
        None => None,
        Some(v) => {
            let name = v.as_str().ok_or("[assistant] worker must be a name")?;
            if name == workers::DEFAULT_WORKER {
                None
            } else if workers.iter().any(|w| w.name == name) {
                Some(name.to_owned())
            } else {
                return Err("[assistant] worker is not configured".into());
            }
        }
    };
    let wanted = match table.get("subscriptions") {
        None => Vec::new(),
        Some(v) => v
            .as_array()
            .and_then(|a| a.iter().map(|n| n.as_str().map(str::to_owned)).collect())
            .ok_or("[assistant] subscriptions must be a list of names")?,
    };
    // It has no Workspace and no connections, so any Claude login may run it.
    let claude: Vec<&str> = all
        .iter()
        .filter(|s| s.agent == WorkerAgent::ClaudeCode)
        .map(|s| s.name.as_str())
        .collect();
    let subscriptions = subscriptions::keep_known(wanted, &claude, "[assistant]");
    if table
        .keys()
        .any(|k| !["enabled", "worker", "subscriptions"].contains(&k.as_str()))
    {
        tracing::warn!(
            "[assistant] keys other than enabled, worker and subscriptions are not used; remove them"
        );
    }
    Ok(Assistant {
        enabled,
        worker,
        subscriptions,
    })
}

/// `assistant/` beside the config file; None when its folder cannot be
/// resolved.
pub fn files(config: &Path) -> Option<SoulFiles> {
    let bound = config.parent()?.canonicalize().ok()?;
    Some(SoulFiles {
        folder: bound.join(DIR),
        bound,
    })
}
