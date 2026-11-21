//! Starting a Task's agent (muxa next B4). Making the place (a worktree) and
//! choosing the agent belong to the host through [`Host`]; the server runs
//! the agent on the backend and finds it again by its labels.

use std::collections::BTreeMap;
use std::future::Future;
use std::path::PathBuf;

use muxa_backend::{Labels, Launch, SessionBackend, TerminalInfo};

/// Labels on an agent terminal muxa started. herdr-core keeps them with the
/// terminal, so a restarted server still finds the Task's agent.
pub const KIND: &str = "muxa.kind";
pub const ROLE: &str = "muxa.role";
pub const DIR: &str = "muxa.dir";
pub const REPO: &str = "muxa.repo";
pub const BRANCH: &str = "muxa.branch";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskSpec {
    pub repo: String,
    pub branch: String,
    pub base: Option<String>,
    pub plan: bool,
}

/// A place ready for the agent, and how to run it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Prepared {
    /// The worktree; the agent starts here.
    pub dir: PathBuf,
    /// The main checkout.
    pub repo: PathBuf,
    pub branch: String,
    pub argv: Vec<String>,
    pub env: BTreeMap<String, String>,
}

#[derive(Debug, thiserror::Error)]
pub enum TaskError {
    /// The request is wrong; the message says what to change.
    #[error("{0}")]
    Invalid(String),
    #[error("{0}")]
    Failed(String),
}

/// What the server asks of the host: Task places and Workspace names.
pub trait Host: Send + Sync {
    /// Checks the request and makes the worktree, or finds the existing one.
    fn prepare(&self, spec: &TaskSpec) -> impl Future<Output = Result<Prepared, TaskError>> + Send;

    /// The Workspace names, for Capture's `#name`.
    fn workspaces(&self) -> impl Future<Output = Vec<String>> + Send {
        async { Vec::new() }
    }
}

/// The Task's live agent terminal, or a new one. `true` when it was running.
pub(crate) async fn start<B: SessionBackend>(
    backend: &B,
    prepared: Prepared,
) -> Result<(TerminalInfo, bool), muxa_backend::BackendError> {
    let labels = labels(&prepared);
    let running = backend
        .list()
        .await?
        .into_iter()
        .find(|t| t.live && labels.iter().all(|(k, v)| t.labels.get(k) == Some(v)));
    if let Some(t) = running {
        return Ok((t, true));
    }
    let launch = Launch {
        argv: prepared.argv,
        cwd: Some(prepared.dir),
        name: None,
        labels,
        env: prepared.env,
    };
    Ok((backend.run(&launch).await?, false))
}

/// What identifies the agent of a Task: its worktree. The repo and branch
/// are for the apps.
fn labels(p: &Prepared) -> Labels {
    Labels::from([
        (KIND.to_owned(), "task".to_owned()),
        (ROLE.to_owned(), "agent".to_owned()),
        (DIR.to_owned(), p.dir.to_string_lossy().into_owned()),
        (REPO.to_owned(), p.repo.to_string_lossy().into_owned()),
        (BRANCH.to_owned(), p.branch.clone()),
    ])
}
