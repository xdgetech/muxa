//! `[assistant]` in `workspaces.toml` (ADR 0084): the Capture assistant,
//! which belongs to no Workspace. Its Soul and Memory live in `assistant/`
//! beside `workspaces.toml`.
use std::path::Path;

use super::bots::SoulFiles;
use super::workers::{self, Worker};

pub const DIR: &str = "assistant";

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Assistant {
    pub enabled: bool,
    /// None: the default agent.
    pub worker: Option<String>,
}

/// Reads `[assistant]`. Keys of the removed assistant layer (ADR 0066) are
/// ignored, so an old file still loads.
pub(super) fn parse(value: Option<&toml::Value>, workers: &[Worker]) -> Result<Assistant, String> {
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
    if table.keys().any(|k| k != "enabled" && k != "worker") {
        tracing::warn!("[assistant] keys other than enabled and worker are not used; remove them");
    }
    Ok(Assistant { enabled, worker })
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
