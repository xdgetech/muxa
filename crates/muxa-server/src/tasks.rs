//! Starting a Task's agent (muxa next B4). Making the places (a folder,
//! worktrees, read-only repos; ADR 0068) and choosing the agent belong to the
//! host through [`Host`]; the server runs the agent on the backend and finds
//! it again by its labels.

use std::collections::BTreeMap;
use std::future::Future;
use std::path::PathBuf;

use muxa_backend::{Labels, Launch, SessionBackend, TerminalInfo};
use muxa_protocol::v2::{Place, PlaceSpec, Routine, RoutineProblem};

/// Labels on an agent terminal muxa started. herdr-core keeps them with the
/// terminal, so a restarted server still finds the Task's agent.
pub const KIND: &str = "muxa.kind";
pub const ROLE: &str = "muxa.role";
pub const DIR: &str = "muxa.dir";
pub const REPO: &str = "muxa.repo";
pub const BRANCH: &str = "muxa.branch";
pub const WORKSPACE: &str = "muxa.workspace";
/// A JSON list of [`Place`], the working place first.
pub const PLACES: &str = "muxa.places";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskSpec {
    pub workspace: String,
    pub title: String,
    pub goal: Option<String>,
    /// Empty: one folder.
    pub places: Vec<PlaceSpec>,
    /// Besides the default ones (ADR 0069).
    pub connections: Vec<String>,
    pub plan: bool,
}

impl TaskSpec {
    /// The first branch place's repo and branch.
    pub fn branch(&self) -> Option<(&str, &str)> {
        self.places.iter().find_map(|p| match p {
            PlaceSpec::Branch { repo, branch, .. } => Some((repo.as_str(), branch.as_str())),
            _ => None,
        })
    }
}

/// The places ready for the agent, and how to run it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Prepared {
    /// The working place: a folder or a worktree. The agent starts here.
    pub dir: PathBuf,
    pub workspace: String,
    pub title: String,
    /// The main checkout and branch of a worktree working place.
    pub repo: Option<PathBuf>,
    pub branch: Option<String>,
    /// Every place, the working place first.
    pub places: Vec<Place>,
    pub argv: Vec<String>,
    pub env: BTreeMap<String, String>,
}

#[derive(Debug, thiserror::Error)]
pub enum TaskError {
    /// The request is wrong; the message says what to change.
    #[error("{0}")]
    Invalid(String),
    /// It cannot happen now, e.g. the Routine already runs.
    #[error("{0}")]
    Busy(String),
    #[error("{0}")]
    Failed(String),
}

/// The host's Routines, and the Workspaces whose Routines cannot be read.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RoutineList {
    pub routines: Vec<Routine>,
    pub problems: Vec<RoutineProblem>,
}

/// What the server asks of the host: Task places and Workspace names.
pub trait Host: Send + Sync {
    /// Checks the request and makes the places, or finds the existing ones.
    fn prepare(&self, spec: &TaskSpec) -> impl Future<Output = Result<Prepared, TaskError>> + Send;

    /// The Workspace names, for Capture's `#name`.
    fn workspaces(&self) -> impl Future<Output = Vec<String>> + Send {
        async { Vec::new() }
    }

    fn routines(&self) -> impl Future<Output = Result<RoutineList, TaskError>> + Send {
        async { Ok(RoutineList::default()) }
    }

    /// Starts a run now on `backend`; it reports through the Inbox like a
    /// scheduled one.
    fn run_routine<B: SessionBackend>(
        &self,
        _backend: &B,
        _workspace: &str,
        _name: &str,
    ) -> impl Future<Output = Result<(), TaskError>> + Send {
        async { Err(TaskError::Invalid("this host runs no Routines".into())) }
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
        .find(|t| t.live && identity(&labels).all(|(k, v)| t.labels.get(k) == Some(v)));
    if let Some(t) = running {
        return Ok((t, true));
    }
    let launch = Launch {
        argv: prepared.argv,
        cwd: Some(prepared.dir),
        name: Some(prepared.title),
        labels,
        env: prepared.env,
    };
    Ok((backend.run(&launch).await?, false))
}

/// What identifies the agent of a Task: its working place. The rest is for
/// the apps.
fn labels(p: &Prepared) -> Labels {
    let mut labels = Labels::from([
        (KIND.to_owned(), "task".to_owned()),
        (ROLE.to_owned(), "agent".to_owned()),
        (DIR.to_owned(), p.dir.to_string_lossy().into_owned()),
        (WORKSPACE.to_owned(), p.workspace.clone()),
    ]);
    if let Some(repo) = &p.repo {
        labels.insert(REPO.to_owned(), repo.to_string_lossy().into_owned());
    }
    if let Some(branch) = &p.branch {
        labels.insert(BRANCH.to_owned(), branch.clone());
    }
    if let Ok(places) = serde_json::to_string(&p.places) {
        labels.insert(PLACES.to_owned(), places);
    }
    labels
}

/// The identifying labels only: a running agent matches on these.
fn identity(labels: &Labels) -> impl Iterator<Item = (&String, &String)> {
    labels
        .iter()
        .filter(|(k, _)| [KIND, ROLE, DIR].contains(&k.as_str()))
}
