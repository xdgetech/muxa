//! Explicit recovery of an interrupted restart, independent of worker support.
use serde::{Deserialize, Serialize};

use crate::WorkerAssignment;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RestartRecoveryAction {
    CancelPrepared,
    FinishReplacement,
    LeaveUnconfirmed,
}

impl RestartRecoveryAction {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::CancelPrepared => "cancel-prepared",
            Self::FinishReplacement => "finish-replacement",
            Self::LeaveUnconfirmed => "leave-unconfirmed",
        }
    }

    pub fn from_wire(value: &str) -> Option<Self> {
        match value {
            "cancel-prepared" => Some(Self::CancelPrepared),
            "finish-replacement" => Some(Self::FinishReplacement),
            "leave-unconfirmed" => Some(Self::LeaveUnconfirmed),
            _ => None,
        }
    }
}

/// Only public recovery metadata; no staged bytes, instructions or environment.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RestartOperation {
    pub id: String,
    /// Future phases remain visible but have no known action.
    pub phase: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub assignment: Option<WorkerAssignment>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub actions: Vec<String>,
}

impl RestartOperation {
    /// Intersect host availability with the client's known phase/action pairs.
    pub fn allows(&self, action: RestartRecoveryAction) -> bool {
        let known = matches!(
            (self.phase.as_str(), action),
            ("prepared", RestartRecoveryAction::CancelPrepared)
                | (
                    "dispatching" | "delivering",
                    RestartRecoveryAction::LeaveUnconfirmed
                )
                | (
                    "replaced",
                    RestartRecoveryAction::FinishReplacement
                        | RestartRecoveryAction::LeaveUnconfirmed
                )
        );
        known && self.actions.iter().any(|a| a == action.as_str())
    }
}
