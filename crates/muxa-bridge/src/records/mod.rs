//! The workspaces config (`workspaces.toml`), its workers, connections and
//! Routines, and the Task Loop's stored form. Independent of the protocol.

pub mod agent_env;
pub mod connections;
pub mod cron;
pub mod lock;
pub mod loops;
pub mod mcp;
pub mod repos;
pub mod routines;
pub mod workers;
pub mod workspaces;

use std::time::{SystemTime, UNIX_EPOCH};

pub use agent_env::AgentSettings;
pub use loops::{Checks, LoopSpec};
pub use workspaces::{Workspace, Workspaces};

const ID_PREFIX: &str = "t-";
const ID_HEX: usize = 6;

/// A run or Task id: `t-` and six lowercase hex digits.
pub fn is_valid_id(id: &str) -> bool {
    id.strip_prefix(ID_PREFIX).is_some_and(|hex| {
        hex.len() == ID_HEX
            && hex
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    })
}

/// Unix seconds now.
pub fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

#[cfg(test)]
mod tests;
