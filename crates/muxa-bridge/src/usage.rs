//! `<state dir>/usage.json`: when an agent last started with each connection
//! and MCP server (ADR 0069 §7). Names and Unix seconds only, never a value.

use std::collections::BTreeMap;
use std::fs::DirBuilder;
use std::os::unix::fs::DirBuilderExt;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::records::Workspace;
use crate::records::lock::Lock;
use crate::routines::store::{StateError, read_json, write_json};

const ID: &str = "usage";
const VERSION: u32 = 1;
const DIR_MODE: u32 = 0o700;
/// Names and times only; far more than any host has.
const MAX_FILE: u64 = 1 << 20;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Usage {
    #[serde(default)]
    version: u32,
    #[serde(default)]
    workspaces: BTreeMap<String, WorkspaceUsage>,
}

/// The last start with each name, by Workspace.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceUsage {
    #[serde(default)]
    pub connections: BTreeMap<String, u64>,
    #[serde(default)]
    pub mcp: BTreeMap<String, u64>,
}

impl Usage {
    pub fn of(&self, workspace: &str) -> WorkspaceUsage {
        self.workspaces.get(workspace).cloned().unwrap_or_default()
    }
}

/// What an agent of `ws` got: the connections `named` and the default ones,
/// with the MCP servers `mcp`, as names for [`record`].
pub fn given(ws: &Workspace, named: &[String], mcp: &[String]) -> (Vec<String>, Vec<String>) {
    let connections = ws
        .connections
        .iter()
        .filter(|c| c.default || named.contains(&c.name))
        .map(|c| c.name.clone())
        .collect();
    (connections, mcp.to_vec())
}

/// Blocking. The file as it is; empty when there is none.
pub fn read(state_dir: &Path) -> Result<Usage, StateError> {
    Ok(read_json(&state_dir.join(format!("{ID}.json")), ID, MAX_FILE)?.unwrap_or_default())
}

/// Blocking, under the file's lock. Notes `at` for each name of `workspace`.
pub fn record(
    state_dir: &Path,
    workspace: &str,
    connections: &[String],
    mcp: &[String],
    at: u64,
) -> Result<(), StateError> {
    if connections.is_empty() && mcp.is_empty() {
        return Ok(());
    }
    DirBuilder::new()
        .recursive(true)
        .mode(DIR_MODE)
        .create(state_dir)
        .map_err(|source| StateError::Io {
            action: "create",
            path: state_dir.to_owned(),
            source,
        })?;
    let _lock = Lock::acquire(state_dir, ID, true)?;
    let mut usage = read(state_dir)?;
    usage.version = VERSION;
    let entry = usage.workspaces.entry(workspace.to_owned()).or_default();
    for name in connections {
        entry.connections.insert(name.clone(), at);
    }
    for name in mcp {
        entry.mcp.insert(name.clone(), at);
    }
    write_json(state_dir, ID, &usage, MAX_FILE)
}

/// [`record`] now; a failure is only logged, since the agent already runs.
pub fn note(state_dir: Option<&Path>, workspace: &str, connections: &[String], mcp: &[String]) {
    let Some(dir) = state_dir else { return };
    if let Err(e) = record(dir, workspace, connections, mcp, crate::records::now()) {
        tracing::warn!(workspace, error = %e, "cannot note when connections were used");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_start_keeps_the_latest_time_per_name_and_nothing_else() {
        let dir = std::env::temp_dir().join(format!(
            "muxa-usage-{}-{:08x}",
            std::process::id(),
            getrandom::u32().unwrap()
        ));
        assert_eq!(read(&dir).unwrap(), Usage::default());
        let names = |n: &[&str]| n.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>();
        record(&dir, "app", &names(&["github"]), &names(&["docs"]), 10).unwrap();
        record(&dir, "app", &names(&["github", "sentry"]), &[], 20).unwrap();
        record(&dir, "other", &[], &names(&["docs"]), 30).unwrap();
        let usage = read(&dir).unwrap();
        let app = usage.of("app");
        assert_eq!(
            app.connections,
            BTreeMap::from([("github".into(), 20), ("sentry".into(), 20)])
        );
        assert_eq!(app.mcp, BTreeMap::from([("docs".into(), 10)]));
        assert_eq!(usage.of("other").mcp["docs"], 30);
        assert_eq!(usage.of("none"), WorkspaceUsage::default());
        let text = std::fs::read_to_string(dir.join("usage.json")).unwrap();
        assert!(text.contains("\"github\": 20"), "{text}");
        let mode = std::fs::metadata(dir.join("usage.json"))
            .unwrap()
            .permissions();
        assert_eq!(
            std::os::unix::fs::PermissionsExt::mode(&mode) & 0o777,
            0o600
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
