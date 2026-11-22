//! `settings` and `settings-update` (ADR 0074): the host's editable view of
//! `workspaces.toml` and each Workspace's `.muxa/routines.toml`, and the ops
//! that change them. Secrets and env values only travel to the server.

use std::collections::BTreeMap;
use std::fmt;

use serde::{Deserialize, Deserializer, Serialize};

use crate::v2::LoopChecks;

/// A value that goes only to the server: a connection's secret or an env
/// value. `Debug` never shows it.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct WriteOnly(pub String);

impl fmt::Debug for WriteOnly {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("<write-only>")
    }
}

impl WriteOnly {
    pub fn expose(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Settings {
    /// A hash of the files' bytes.
    pub revision: String,
    pub read_only: bool,
    /// Why `workspaces.toml` cannot be used as it is now.
    pub problem: Option<String>,
    pub workspaces: Vec<SettingsWorkspace>,
    pub workers: Vec<WorkerSpec>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SettingsWorkspace {
    pub name: String,
    /// As written, `~/` included.
    pub root: String,
    #[serde(default)]
    pub repos: Vec<String>,
    /// Repos under the root, found by the scan (ADR 0031), hidden ones too,
    /// as named under the root. Not in the file.
    #[serde(default)]
    pub found_repos: Vec<String>,
    /// Found repos the Workspace leaves out of its repo list.
    #[serde(default)]
    pub hidden_repos: Vec<String>,
    #[serde(default)]
    pub park_after: Option<String>,
    pub agent: SettingsAgent,
    #[serde(rename = "loop", default)]
    pub task_loop: Option<SettingsLoop>,
    #[serde(default)]
    pub routines_enabled: bool,
    #[serde(default)]
    pub direct_writes: bool,
    #[serde(default)]
    pub connections: Vec<SettingsConnection>,
    #[serde(default)]
    pub mcp: Vec<McpSpec>,
    #[serde(default)]
    pub routines: Vec<SettingsRoutine>,
    /// Why `.muxa/routines.toml` cannot be used as it is now.
    #[serde(default)]
    pub routines_problem: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SettingsAgent {
    /// None: the daemon's `--task-command`.
    #[serde(default)]
    pub command: Option<String>,
    /// The keys only; a value never leaves the host.
    #[serde(default)]
    pub env: Vec<EnvKey>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EnvKey {
    pub key: String,
    pub set: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SettingsLoop {
    pub checks: LoopChecks,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Access {
    Read,
    Write,
}

/// Whether a connection's Keychain secret exists; never the secret.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SecretState {
    Set,
    Missing,
    /// The host's Keychain is locked.
    Locked,
    /// The host has no macOS Keychain.
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SettingsConnection {
    #[serde(flatten)]
    pub spec: ConnectionSpec,
    pub secret: SecretState,
}

/// `[[workspace.connection]]` without its secret. Every agent of the
/// Workspace gets it (ADR 0075); there is no `default`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConnectionSpec {
    pub name: String,
    pub env: String,
    #[serde(default)]
    pub account: Option<String>,
    #[serde(default)]
    pub access: Option<Access>,
}

/// `[[workspace.mcp]]`. Nothing here is a secret. Every agent of the
/// Workspace gets it (ADR 0075); there is no `default`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct McpSpec {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub args: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(rename = "type", default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    #[serde(default)]
    pub headers: BTreeMap<String, String>,
    #[serde(default)]
    pub connections: Vec<String>,
}

/// `[[routine]]` with `[routine.output]` flattened. A run gets all its
/// Workspace's connections and MCP servers (ADR 0075).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoutineSpec {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub schedule: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timezone: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub worker: Option<String>,
    /// Relative to `.muxa/`.
    pub instructions: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project: Option<String>,
    pub enabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub items: Option<u32>,
    #[serde(default)]
    pub direct_writes: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SettingsRoutine {
    #[serde(flatten)]
    pub spec: RoutineSpec,
    /// The instruction file's content; None when it cannot be read.
    #[serde(default)]
    pub instructions_text: Option<String>,
}

/// `[[worker]]`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkerSpec {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub agent: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub instructions: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspaces: Option<Vec<String>>,
    #[serde(rename = "loop", default, skip_serializing_if = "Option::is_none")]
    pub task_loop: Option<WorkerLoop>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkerLoop {
    pub check: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_attempts: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout: Option<u64>,
}

/// Present (even as `null`) or absent: `null` clears, absent keeps.
fn present<'de, D: Deserializer<'de>, T: Deserialize<'de>>(d: D) -> Result<Option<T>, D::Error> {
    T::deserialize(d).map(Some)
}

/// One change of `settings-update`. Names are never renamed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "kebab-case")]
pub enum SettingsOp {
    WorkspaceAdd {
        name: String,
        root: String,
        #[serde(default)]
        repos: Vec<String>,
    },
    WorkspaceUpdate {
        workspace: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        root: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        repos: Option<Vec<String>>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        hidden_repos: Option<Vec<String>>,
        #[serde(
            default,
            deserialize_with = "present",
            skip_serializing_if = "Option::is_none"
        )]
        agent_command: Option<Option<String>>,
        #[serde(
            default,
            deserialize_with = "present",
            skip_serializing_if = "Option::is_none"
        )]
        park_after: Option<Option<String>>,
    },
    WorkspaceRemove {
        workspace: String,
    },
    EnvSet {
        workspace: String,
        key: String,
        value: WriteOnly,
    },
    EnvUnset {
        workspace: String,
        key: String,
    },
    LoopChecks {
        workspace: String,
        checks: Option<LoopChecks>,
    },
    RoutinesGate {
        workspace: String,
        enabled: bool,
        #[serde(default)]
        direct_writes: bool,
    },
    ConnectionAdd {
        workspace: String,
        connection: ConnectionSpec,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        secret: Option<WriteOnly>,
    },
    ConnectionUpdate {
        workspace: String,
        connection: ConnectionSpec,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        secret: Option<WriteOnly>,
    },
    ConnectionSecret {
        workspace: String,
        name: String,
        secret: WriteOnly,
    },
    ConnectionRemove {
        workspace: String,
        name: String,
    },
    McpAdd {
        workspace: String,
        mcp: McpSpec,
    },
    McpUpdate {
        workspace: String,
        mcp: McpSpec,
    },
    McpRemove {
        workspace: String,
        name: String,
    },
    RoutineAdd {
        workspace: String,
        routine: RoutineSpec,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        instructions_text: Option<String>,
    },
    RoutineUpdate {
        workspace: String,
        routine: RoutineSpec,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        instructions_text: Option<String>,
    },
    RoutineRemove {
        workspace: String,
        name: String,
    },
    WorkerAdd {
        worker: WorkerSpec,
    },
    WorkerUpdate {
        worker: WorkerSpec,
    },
    WorkerRemove {
        name: String,
    },
}

impl SettingsOp {
    /// The `op` tag, for error messages.
    pub fn name(&self) -> &'static str {
        match self {
            Self::WorkspaceAdd { .. } => "workspace-add",
            Self::WorkspaceUpdate { .. } => "workspace-update",
            Self::WorkspaceRemove { .. } => "workspace-remove",
            Self::EnvSet { .. } => "env-set",
            Self::EnvUnset { .. } => "env-unset",
            Self::LoopChecks { .. } => "loop-checks",
            Self::RoutinesGate { .. } => "routines-gate",
            Self::ConnectionAdd { .. } => "connection-add",
            Self::ConnectionUpdate { .. } => "connection-update",
            Self::ConnectionSecret { .. } => "connection-secret",
            Self::ConnectionRemove { .. } => "connection-remove",
            Self::McpAdd { .. } => "mcp-add",
            Self::McpUpdate { .. } => "mcp-update",
            Self::McpRemove { .. } => "mcp-remove",
            Self::RoutineAdd { .. } => "routine-add",
            Self::RoutineUpdate { .. } => "routine-update",
            Self::RoutineRemove { .. } => "routine-remove",
            Self::WorkerAdd { .. } => "worker-add",
            Self::WorkerUpdate { .. } => "worker-update",
            Self::WorkerRemove { .. } => "worker-remove",
        }
    }

    /// The Workspace the op changes; None for a worker op.
    pub fn workspace(&self) -> Option<&str> {
        match self {
            Self::WorkspaceAdd { name, .. } => Some(name),
            Self::WorkspaceUpdate { workspace, .. }
            | Self::WorkspaceRemove { workspace }
            | Self::EnvSet { workspace, .. }
            | Self::EnvUnset { workspace, .. }
            | Self::LoopChecks { workspace, .. }
            | Self::RoutinesGate { workspace, .. }
            | Self::ConnectionAdd { workspace, .. }
            | Self::ConnectionUpdate { workspace, .. }
            | Self::ConnectionSecret { workspace, .. }
            | Self::ConnectionRemove { workspace, .. }
            | Self::McpAdd { workspace, .. }
            | Self::McpUpdate { workspace, .. }
            | Self::McpRemove { workspace, .. }
            | Self::RoutineAdd { workspace, .. }
            | Self::RoutineUpdate { workspace, .. }
            | Self::RoutineRemove { workspace, .. } => Some(workspace),
            Self::WorkerAdd { .. } | Self::WorkerUpdate { .. } | Self::WorkerRemove { .. } => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn absent_keeps_and_null_clears() {
        let keep: SettingsOp =
            serde_json::from_value(json!({"op": "workspace-update", "workspace": "w"})).unwrap();
        let clear: SettingsOp = serde_json::from_value(
            json!({"op": "workspace-update", "workspace": "w", "agent_command": null, "park_after": "2d"}),
        )
        .unwrap();
        let SettingsOp::WorkspaceUpdate {
            agent_command,
            park_after,
            ..
        } = keep
        else {
            panic!("not an update")
        };
        assert_eq!((agent_command, park_after), (None, None));
        let SettingsOp::WorkspaceUpdate {
            agent_command,
            park_after,
            ..
        } = clear.clone()
        else {
            panic!("not an update")
        };
        assert_eq!(agent_command, Some(None));
        assert_eq!(park_after, Some(Some("2d".into())));
        assert_eq!(
            serde_json::to_value(&clear).unwrap(),
            json!({"op": "workspace-update", "workspace": "w", "agent_command": null, "park_after": "2d"})
        );
    }

    #[test]
    fn write_only_values_never_show_in_debug() {
        let op: SettingsOp = serde_json::from_value(json!({"op": "connection-add",
            "workspace": "w", "connection": {"name": "gh", "env": "GH_TOKEN"}, "secret": "s3cret"}))
        .unwrap();
        let env: SettingsOp = serde_json::from_value(
            json!({"op": "env-set", "workspace": "w", "key": "K", "value": "v4lue"}),
        )
        .unwrap();
        let shown = format!("{op:?} {env:?}");
        assert!(
            !shown.contains("s3cret") && !shown.contains("v4lue"),
            "{shown}"
        );
        assert_eq!((op.name(), op.workspace()), ("connection-add", Some("w")));
    }

    #[test]
    fn a_connection_shows_its_secret_state_beside_its_fields() {
        let c = SettingsConnection {
            spec: ConnectionSpec {
                name: "gh".into(),
                env: "GH_TOKEN".into(),
                account: None,
                access: Some(Access::Read),
            },
            secret: SecretState::Missing,
        };
        assert_eq!(
            serde_json::to_value(&c).unwrap(),
            json!({"name": "gh", "env": "GH_TOKEN", "account": null, "access": "read",
                "secret": "missing"})
        );
    }
}
