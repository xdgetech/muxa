//! Worker selection is intent; agent identity still comes from an adapter.
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::LoopSpec;

/// Missing means Keep; JSON null means Workspace default. Never collapse them.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub enum WorkerSelection {
    #[default]
    Keep,
    WorkspaceDefault,
    Named(String),
}

impl WorkerSelection {
    pub fn is_keep(&self) -> bool {
        matches!(self, Self::Keep)
    }
}

impl Serialize for WorkerSelection {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Named(name) => serializer.serialize_str(name),
            Self::WorkspaceDefault => serializer.serialize_none(),
            Self::Keep => Err(serde::ser::Error::custom(
                "Keep must be omitted by the enclosing request",
            )),
        }
    }
}

impl<'de> Deserialize<'de> for WorkerSelection {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Ok(match Option::<String>::deserialize(deserializer)? {
            Some(name) => Self::Named(name),
            None => Self::WorkspaceDefault,
        })
    }
}

/// A run's requested assignment, not evidence of the effective native role.
/// Missing on a legacy run means unrecorded. Unknown kinds have no known meaning.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkerAssignment {
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

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

impl WorkerAssignment {
    pub fn is_known(&self) -> bool {
        match self.kind.as_str() {
            "workspace-default" => self.name.is_none(),
            "worker" => self.name.as_ref().is_some_and(|name| !name.is_empty()),
            _ => false,
        }
    }
}
