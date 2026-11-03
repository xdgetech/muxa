//! Worker selection is intent; agent identity still comes from an adapter.
use serde::{Deserialize, Serialize};

use crate::LoopSpec;

/// The reserved worker name for the workspace default agent (ADR 0027).
/// A host worker may not use it.
pub const DEFAULT_WORKER: &str = "default";

/// Public host-owned worker metadata. Args, instructions and env never travel here.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkerInfo {
    pub name: String,
    pub agent: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Missing allows every workspace; an empty list allows none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspaces: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub r#loop: Option<LoopSpec>,
}
