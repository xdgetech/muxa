//! Task records: the durable Task, one JSON file per record on the host, and
//! the workspaces config. Independent of the protocol and the server, so a
//! later daemon can link it.

pub mod agent_env;
pub mod inbox;
pub mod lifecycle;
pub mod loops;
pub mod observation;
pub mod prompts;
pub mod restart_files;
pub mod runs;
pub mod store;
pub mod workers;
pub mod workspaces;

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

pub use agent_env::AgentSettings;
pub use loops::{Checks, LastCheck, Loop, LoopSpec, LoopState, Reason};
pub use prompts::{PendingPrompt, PromptKind};
pub use runs::{MAX_RUNS, Run, RunReason, Seen};
pub use store::{Store, StoreError, now};
pub use workspaces::{Workspace, Workspaces};

/// The record file format this code writes, and the newest it reads.
pub const FORMAT_VERSION: u32 = 1;
/// In characters.
pub const MAX_TITLE: usize = 200;
/// In bytes.
pub const MAX_GOAL: usize = 16 * 1024;
/// Any other text field, in bytes.
pub const MAX_FIELD: usize = 4096;
/// A whole record file, in bytes.
pub const MAX_FILE: u64 = 64 * 1024;

const ID_PREFIX: &str = "t-";
const ID_HEX: usize = 6;

/// A Task, as stored in `<id>.json`. Times are Unix epoch seconds.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Record {
    pub version: u32,
    pub id: String,
    /// Incremented on every write.
    pub rev: u64,
    pub workspace: String,
    pub title: String,
    pub goal: String,
    pub status: Status,
    pub created_at: u64,
    pub updated_at: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repo: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    /// The task directory or worktree.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dir: Option<PathBuf>,
    /// The last known session name, for display. Sessions link by their
    /// `@muxa_task` tag, or by `dir` (ADR 0015).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    /// The agents the bridge started or restarted, oldest first (ADR 0018).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub runs: Vec<Run>,
    /// Fields written by a newer muxa; kept on rewrite.
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

/// Set by the user; separate from the agent's state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    Open,
    Review,
    Done,
    Dropped,
}

/// What a client gives to create a record.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NewRecord {
    pub workspace: String,
    pub title: String,
    pub goal: String,
    pub project: Option<String>,
    pub repo: Option<PathBuf>,
    pub branch: Option<String>,
    pub dir: Option<PathBuf>,
    pub session: Option<String>,
    pub note: Option<String>,
    pub worker: Option<String>,
}

/// `t-` and 6 lowercase hex digits. Ids become file names, so nothing else passes.
pub fn is_valid_id(id: &str) -> bool {
    id.strip_prefix(ID_PREFIX).is_some_and(|hex| {
        hex.len() == ID_HEX
            && hex
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    })
}

/// A fresh id from 24 random bits.
fn new_id() -> Result<String, getrandom::Error> {
    let bits = getrandom::u32()? & 0x00ff_ffff;
    Ok(format!("{ID_PREFIX}{bits:06x}"))
}

#[cfg(test)]
mod tests;

impl Record {
    /// W1 reads the additive field without changing older-record rewrite behavior.
    pub fn worker(&self) -> Option<&str> {
        self.extra.get("worker").and_then(serde_json::Value::as_str)
    }
}
