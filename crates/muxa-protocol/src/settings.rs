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
    /// The Capture assistant (ADR 0084): `[assistant]` in `workspaces.toml`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub assistant: Option<SettingsAssistant>,
    /// `[[subscription]]`: named agent logins (ADR 0103).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub subscriptions: Vec<SubscriptionSpec>,
    /// Logins on the host that are no subscription yet, for onboarding
    /// (ADR 0103 §7).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub found_logins: Vec<FoundLogin>,
}

/// A login muxa found without opening a file: a default folder that exists,
/// or a Workspace's env variable. An env value never crosses the wire, so a
/// Workspace's login has no `home`; `subscription-adopt` makes it one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FoundLogin {
    /// `claude-code` or `codex`.
    pub agent: String,
    /// `~/.claude` or `~/.codex`; absent for a Workspace's env login.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub home: Option<String>,
    /// The Workspace whose env names this login.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace: Option<String>,
}

/// `[[subscription]]` (ADR 0103): an agent login muxa names. The login
/// itself stays with the agent; muxa keeps the name and the folder.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SubscriptionSpec {
    pub name: String,
    /// `claude-code` or `codex`.
    pub agent: String,
    /// As written, `~/` included.
    pub home: String,
    /// A label only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plan: Option<String>,
}

/// The Capture assistant: no Workspace, no connections (ADR 0084).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SettingsAssistant {
    pub enabled: bool,
    /// None: the default agent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub worker: Option<String>,
    /// `assistant/soul.md` beside `workspaces.toml`; None when it cannot be
    /// read, empty when there is none.
    #[serde(default)]
    pub soul_text: Option<String>,
    /// `assistant/memory.md`, read only: the assistant writes it.
    #[serde(default)]
    pub memory_text: Option<String>,
    /// The Claude subscriptions it runs on, in order (ADR 0103).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub subscriptions: Vec<String>,
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
    /// The user's claude.ai connectors load too, beside the Workspace's MCP
    /// servers (ADR 0087).
    #[serde(default)]
    pub claude_connectors: bool,
    /// Claude Code permission rules for this Workspace's Bots only (ADR 0087).
    #[serde(default, skip_serializing_if = "BotRules::is_empty")]
    pub bot_rules: BotRules,
    /// The subscriptions its agents use, in order (ADR 0103).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub subscriptions: Vec<String>,
    /// False: its agents keep the first login of their kind, also when it
    /// is full; the user switches by the order (ADR 0103 §4).
    #[serde(default = "yes", skip_serializing_if = "is_yes")]
    pub switch_subscriptions: bool,
    #[serde(default)]
    pub connections: Vec<SettingsConnection>,
    #[serde(default)]
    pub mcp: Vec<McpSpec>,
    #[serde(default)]
    pub routines: Vec<SettingsRoutine>,
    /// The `[[bot]]` tables of `.muxa/routines.toml` (ADR 0082).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub bots: Vec<SettingsBot>,
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
    /// The `[[bot]]` that owns it, in the same file (ADR 0082).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bot: Option<String>,
    /// Claude Code's edit tools are denied to the run's agent.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub read_only: bool,
    /// An event that runs it (ADR 0084): only `review`, a Task of its
    /// Workspace asking for review.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub on: Option<String>,
}

/// `[[bot]]` (ADR 0082). Its Soul and Memory live in `.muxa/bots/<name>/`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BotSpec {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub worker: Option<String>,
    /// None: 3.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub daily_items: Option<u32>,
    pub enabled: bool,
    /// A team lead's members (ADR 0085): Bots of the same Workspace that
    /// lead no team themselves.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub members: Vec<String>,
    /// A lead's handoffs a day, all members together. None: 10.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub handoffs_per_day: Option<u32>,
    /// Shell commands only this Bot may run, as Claude Code `Bash(...)` patterns
    /// without the wrapper, e.g. `/path/collect.sh delivery:*` (ADR 0088).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub commands: Vec<String>,
    /// Its own order of its Workspace's subscriptions (ADR 0103). Empty:
    /// the Workspace's.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub subscriptions: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SettingsBot {
    #[serde(flatten)]
    pub spec: BotSpec,
    /// `soul.md`; None when it cannot be read, empty when there is none.
    #[serde(default)]
    pub soul_text: Option<String>,
    /// `memory.md`, read only: the Bot writes it.
    #[serde(default)]
    pub memory_text: Option<String>,
    /// How many earlier memories `bot-memory-restore` can bring back.
    #[serde(default)]
    pub memory_versions: u32,
    /// When each earlier memory was saved, newest first (ADR 0086).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub memory_history: Vec<MemoryVersion>,
}

/// `[workspace.bots]`: Claude Code permission rules, as in its settings file.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct BotRules {
    #[serde(default)]
    pub allow: Vec<String>,
    #[serde(default)]
    pub deny: Vec<String>,
}

impl BotRules {
    pub fn is_empty(&self) -> bool {
        self.allow.is_empty() && self.deny.is_empty()
    }
}

/// One earlier memory `bot-memory-restore` can bring back.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryVersion {
    /// 1 is the newest.
    pub version: u32,
    /// Unix seconds when it was replaced.
    pub at: i64,
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
        #[serde(default, skip_serializing_if = "Option::is_none")]
        claude_connectors: Option<bool>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        bot_rules: Option<BotRules>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        subscriptions: Option<Vec<String>>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        switch_subscriptions: Option<bool>,
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
    BotAdd {
        workspace: String,
        bot: BotSpec,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        soul_text: Option<String>,
    },
    BotUpdate {
        workspace: String,
        bot: BotSpec,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        soul_text: Option<String>,
    },
    /// Its Routines must go first; its folder stays.
    BotRemove {
        workspace: String,
        name: String,
    },
    /// Renames a Bot: its table, the Routines and teams that name it, and its
    /// folder with the Soul and Memory (ADR 0089).
    BotRename {
        workspace: String,
        from: String,
        to: String,
    },
    /// Brings back kept memory `version` (1 is the newest); the current one
    /// is kept in its place.
    BotMemoryRestore {
        workspace: String,
        name: String,
        version: u32,
    },
    /// The Capture assistant's settings and Soul (ADR 0084).
    AssistantUpdate {
        enabled: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        worker: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        soul_text: Option<String>,
        /// Absent keeps the list; present replaces it.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        subscriptions: Option<Vec<String>>,
    },
    /// A named agent login (ADR 0103).
    SubscriptionAdd {
        subscription: SubscriptionSpec,
    },
    SubscriptionUpdate {
        subscription: SubscriptionSpec,
    },
    /// Refused while a Workspace lists it.
    SubscriptionRemove {
        name: String,
    },
    /// Makes the login a Workspace's env names a subscription called `name`
    /// (one with that folder already is reused), takes the variable out of
    /// the env, and puts the subscription first in the Workspace's list.
    SubscriptionAdopt {
        workspace: String,
        /// `claude-code` or `codex`: which variable.
        agent: String,
        name: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        plan: Option<String>,
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
            Self::BotAdd { .. } => "bot-add",
            Self::BotUpdate { .. } => "bot-update",
            Self::BotRemove { .. } => "bot-remove",
            Self::BotRename { .. } => "bot-rename",
            Self::BotMemoryRestore { .. } => "bot-memory-restore",
            Self::AssistantUpdate { .. } => "assistant-update",
            Self::SubscriptionAdd { .. } => "subscription-add",
            Self::SubscriptionUpdate { .. } => "subscription-update",
            Self::SubscriptionRemove { .. } => "subscription-remove",
            Self::SubscriptionAdopt { .. } => "subscription-adopt",
            Self::WorkerAdd { .. } => "worker-add",
            Self::WorkerUpdate { .. } => "worker-update",
            Self::WorkerRemove { .. } => "worker-remove",
        }
    }

    /// The Workspace the op changes; None for a worker or assistant op.
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
            | Self::RoutineRemove { workspace, .. }
            | Self::BotAdd { workspace, .. }
            | Self::BotUpdate { workspace, .. }
            | Self::BotRemove { workspace, .. }
            | Self::BotRename { workspace, .. }
            | Self::BotMemoryRestore { workspace, .. }
            | Self::SubscriptionAdopt { workspace, .. } => Some(workspace),
            Self::AssistantUpdate { .. }
            | Self::SubscriptionAdd { .. }
            | Self::SubscriptionUpdate { .. }
            | Self::SubscriptionRemove { .. }
            | Self::WorkerAdd { .. }
            | Self::WorkerUpdate { .. }
            | Self::WorkerRemove { .. } => None,
        }
    }
}

fn yes() -> bool {
    true
}

fn is_yes(v: &bool) -> bool {
    *v
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
