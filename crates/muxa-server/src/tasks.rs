//! Starting a Task's agent (muxa next B4). Making the places (a folder,
//! worktrees, read-only repos; ADR 0068) and choosing the agent belong to the
//! host through [`Host`]; the server runs the agent on the backend and finds
//! it again by its labels.

use std::collections::BTreeMap;
use std::future::Future;
use std::path::{Path, PathBuf};

use muxa_backend::{Labels, Launch, SessionBackend, TerminalInfo};
use muxa_protocol::v2::{
    ChatUpdate, LoopInfo, LoopSpec, Place, PlaceSpec, ReaderContent, ReaderKind, ReaderList,
    Routine, RoutineProblem, Settings, SettingsOp, V1Task, Workspace, WorktreeMode,
};
use tokio::sync::watch;

use crate::reader::ReaderOpen;

/// Labels on an agent terminal muxa started. herdr-core keeps them with the
/// terminal, so a restarted server still finds the Task's agent.
pub const KIND: &str = "muxa.kind";
pub const ROLE: &str = "muxa.role";
pub const DIR: &str = "muxa.dir";
pub const REPO: &str = "muxa.repo";
pub const BRANCH: &str = "muxa.branch";
pub const WORKSPACE: &str = "muxa.workspace";
/// The Task's title as the user wrote it. The terminal's name is a slug of
/// it, since herdr-core takes only `a-z 0-9 - _ .`.
pub const TITLE: &str = "muxa.title";
/// A JSON list of [`Place`], the working place first.
pub const PLACES: &str = "muxa.places";
/// A JSON list of the connections the agent got: all its Workspace's (ADR
/// 0075). Names only.
pub const CONNECTIONS: &str = "muxa.connections";
/// A JSON list of every MCP server the agent got. Names only.
pub const MCP: &str = "muxa.mcp";
/// The repo of a `checkout` Task as the request named it: a second start
/// finds the running agent by it.
pub const CHECKOUT: &str = "muxa.checkout";
/// The Bot a talk Task talks with (ADR 0083).
pub const BOT: &str = "muxa.bot";
/// The subscription, a named login, the agent runs on (ADR 0091). A restart
/// keeps it: the conversation lives in that login's folder.
pub const SUBSCRIPTION: &str = "muxa.subscription";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskSpec {
    pub workspace: String,
    pub title: String,
    pub goal: Option<String>,
    /// Empty: one folder.
    pub places: Vec<PlaceSpec>,
    /// Deprecated and ignored (ADR 0075): every agent gets them all.
    pub connections: Vec<String>,
    /// Deprecated and ignored (ADR 0075).
    pub mcp: Vec<String>,
    pub plan: bool,
    /// Talk to this Bot of the Workspace (ADR 0083).
    pub bot: Option<String>,
}

impl TaskSpec {
    /// The first branch place's repo and branch.
    pub fn branch(&self) -> Option<(&str, &str)> {
        self.places.iter().find_map(|p| match p {
            PlaceSpec::Branch { repo, branch, .. } => Some((repo.as_str(), branch.as_str())),
            _ => None,
        })
    }

    /// The path of an `existing` place: the working place it adopts.
    pub fn existing(&self) -> Option<&str> {
        self.places.iter().find_map(|p| match p {
            PlaceSpec::Existing { path } => Some(path.as_str()),
            _ => None,
        })
    }

    /// The repo of a `checkout` place, as the request names it.
    pub fn checkout(&self) -> Option<&str> {
        self.places.iter().find_map(|p| match p {
            PlaceSpec::Checkout { repo } => Some(repo.as_str()),
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
    /// The repo a `checkout` Task names, as the request gave it.
    pub checkout: Option<String>,
    /// Every place, the working place first.
    pub places: Vec<Place>,
    /// Every connection the agent gets: all its Workspace's (ADR 0075).
    pub connections: Vec<String>,
    /// Every MCP server the agent gets: all its Workspace's.
    pub mcp: Vec<String>,
    pub argv: Vec<String>,
    pub env: BTreeMap<String, String>,
    /// The Bot of a talk Task (ADR 0083).
    pub bot: Option<String>,
    /// The subscription the agent runs on (ADR 0091).
    pub subscription: Option<String>,
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
    /// Closing a Task would lose work; the message says which.
    #[error("{0}")]
    WouldLoseWork(String),
    /// The settings changed since the client read them.
    #[error("{0}")]
    Stale(String),
    /// The host locked its settings.
    #[error("{0}")]
    ReadOnly(String),
}

/// The host's Routines, and the Workspaces whose Routines cannot be read.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RoutineList {
    pub routines: Vec<Routine>,
    pub problems: Vec<RoutineProblem>,
    pub off: Vec<muxa_protocol::v2::RoutinesOff>,
}

/// What the server asks of the host: Task places and Workspace names.
pub trait Host: Send + Sync {
    /// Checks the request and makes the places, or finds the existing ones.
    fn prepare(&self, spec: &TaskSpec) -> impl Future<Output = Result<Prepared, TaskError>> + Send;

    /// Writes an uploaded file into `dir/.muxa/uploads/` (ADR 0081) and returns
    /// its path relative to `dir`. `name` is already a checked file name; a
    /// taken one gets a number.
    fn upload(
        &self,
        _dir: &Path,
        _name: &str,
        _bytes: Vec<u8>,
    ) -> impl Future<Output = Result<String, TaskError>> + Send {
        async { Err(TaskError::Invalid("this host takes no uploads".into())) }
    }

    /// The program of a Task's shell (ADR 0079), as argv.
    fn shell(&self) -> Vec<String> {
        vec!["/bin/sh".to_owned(), "-l".to_owned()]
    }

    /// The Workspace names, for Capture's `#name`.
    fn workspaces(&self) -> impl Future<Output = Vec<String>> + Send {
        async { Vec::new() }
    }

    /// The Workspaces as `workspaces` shows them. Nothing secret.
    fn workspace_list(&self) -> impl Future<Output = Result<Vec<Workspace>, TaskError>> + Send {
        async { Ok(Vec::new()) }
    }

    /// What runs for `subscription`'s login terminal (ADR 0091 §7).
    fn login_launch(
        &self,
        _subscription: &str,
    ) -> impl Future<Output = Result<crate::logins::LoginLaunch, TaskError>> + Send {
        async { Err(TaskError::Invalid("this host has no subscriptions".into())) }
    }

    /// Each subscription and the limits its agents last reported (ADR 0091).
    fn subscriptions(
        &self,
    ) -> impl Future<Output = Result<Vec<muxa_protocol::v2::SubscriptionUsage>, TaskError>> + Send
    {
        async { Ok(Vec::new()) }
    }

    /// The tokens agents used on `day` (`YYYY-MM-DD`, else today), by
    /// Workspace and by Task (ADR 0091 §6).
    fn token_usage(
        &self,
        _day: Option<String>,
    ) -> impl Future<Output = Result<muxa_protocol::v2::TokenUsage, TaskError>> + Send {
        async { Err(TaskError::Invalid("this host counts no tokens".into())) }
    }

    /// Whether closing a Task with `places` may go on: with
    /// [`WorktreeMode::Remove`], [`TaskError::WouldLoseWork`] when a branch
    /// worktree has uncommitted or unmerged work. Changes nothing.
    fn check_close(
        &self,
        _places: &[Place],
        _mode: WorktreeMode,
    ) -> impl Future<Output = Result<(), TaskError>> + Send {
        async { Ok(()) }
    }

    /// Removes the branch worktrees of a closed Task (ADR 0068 §7, ADR 0043).
    /// Never a read-only place or the folder.
    fn close_places(
        &self,
        _places: &[Place],
        _mode: WorktreeMode,
    ) -> impl Future<Output = Result<(), TaskError>> + Send {
        async { Ok(()) }
    }

    /// A new agent runs with `prepared`, after a Task start or a restart.
    /// The host notes when its connections and MCP servers were last used.
    fn agent_started(&self, _prepared: &Prepared) -> impl Future<Output = ()> + Send {
        async {}
    }

    /// How to run the agent of `ended`, a Task's agent terminal, again in its
    /// working place, with the same places and connections.
    fn restart(
        &self,
        _ended: &TerminalInfo,
    ) -> impl Future<Output = Result<Prepared, TaskError>> + Send {
        async { Err(TaskError::Invalid("this host cannot restart Tasks".into())) }
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

    /// Checks a Task Loop for `workspace` before its Task starts.
    fn loop_check(
        &self,
        _workspace: &str,
        _spec: &LoopSpec,
    ) -> impl Future<Output = Result<(), TaskError>> + Send {
        async { Err(TaskError::Invalid("this host runs no Task Loops".into())) }
    }

    /// Starts the Task Loop of `agent`, a Task's agent terminal.
    fn loop_start(
        &self,
        _agent: &TerminalInfo,
        _spec: &LoopSpec,
        _reset: bool,
    ) -> impl Future<Output = Result<LoopInfo, TaskError>> + Send {
        async { Err(TaskError::Invalid("this host runs no Task Loops".into())) }
    }

    fn loop_stop(
        &self,
        _terminal: &str,
    ) -> impl Future<Output = Result<LoopInfo, TaskError>> + Send {
        async { Err(TaskError::Invalid("this host runs no Task Loops".into())) }
    }

    /// The loops of the live Task agents.
    fn loops(&self) -> impl Future<Output = Vec<LoopInfo>> + Send {
        async { Vec::new() }
    }

    /// Bumped when a loop changes, for `watch`. None: no loops here.
    fn loop_changes(&self) -> Option<watch::Receiver<u64>> {
        None
    }

    /// Opens the conversation of the agent working in `dir`: a handle for
    /// [`Host::chat_poll`], and everything it holds now.
    fn chat_open(
        &self,
        _dir: &Path,
    ) -> impl Future<Output = Result<(u64, ChatUpdate), TaskError>> + Send {
        async { Err(TaskError::Invalid("this host has no Chat".into())) }
    }

    /// What changed since the last poll; None when nothing did. An error
    /// ends the chat, and its message says why.
    fn chat_poll(
        &self,
        _handle: u64,
    ) -> impl Future<Output = Result<Option<ChatUpdate>, TaskError>> + Send {
        async { Err(TaskError::Invalid("this host has no Chat".into())) }
    }

    fn chat_close(&self, _handle: u64) -> impl Future<Output = ()> + Send {
        async {}
    }

    /// The open Tasks of protocol v1 (ADR 0076), each `new` when its place
    /// can be adopted, else why not. Changes nothing.
    fn v1_tasks(&self) -> impl Future<Output = Result<Vec<V1Task>, TaskError>> + Send {
        async { Ok(Vec::new()) }
    }

    /// The host's editable settings (ADR 0074). Never a secret or env value.
    fn settings(&self) -> impl Future<Output = Result<Settings, TaskError>> + Send {
        async { Err(TaskError::Invalid("this host has no settings".into())) }
    }

    /// Applies `ops`, all or none, when `revision` is still the current one;
    /// else [`TaskError::Stale`]. The settings after them.
    fn settings_update(
        &self,
        _revision: &str,
        _ops: Vec<SettingsOp>,
    ) -> impl Future<Output = Result<Settings, TaskError>> + Send {
        async { Err(TaskError::Invalid("this host has no settings".into())) }
    }

    /// The files of `kind` in `dir` the Reader may open. Nothing else is served.
    fn reader_list(
        &self,
        _dir: &Path,
        _kind: ReaderKind,
        _html: bool,
    ) -> impl Future<Output = Result<ReaderList, TaskError>> + Send {
        async { Err(TaskError::Invalid("this host has no Reader".into())) }
    }

    /// A listed file of `dir` as it is now. Called again while it is open, so
    /// it checks again that the file is listed.
    /// The user accepted or dismissed the Capture assistant's suggestion on
    /// `item` (ADR 0084), so the assistant can learn from it.
    fn suggestion_decided(
        &self,
        _item: &muxa_protocol::v2::InboxItem,
        _suggestion: &muxa_protocol::v2::ItemSuggestion,
        _accepted: bool,
    ) -> impl Future<Output = ()> + Send {
        async {}
    }

    fn reader_read(
        &self,
        _dir: &Path,
        _open: &ReaderOpen,
    ) -> impl Future<Output = Result<ReaderContent, TaskError>> + Send {
        async { Err(TaskError::Invalid("this host has no Reader".into())) }
    }
}

/// The Task's live agent terminal, or a new one. `true` when it was running.
pub(crate) async fn start<B: SessionBackend, H: Host>(
    backend: &B,
    host: &H,
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
        argv: prepared.argv.clone(),
        cwd: Some(prepared.dir.clone()),
        name: Some(prepared.title.clone()),
        labels,
        env: prepared.env.clone(),
    };
    let started = backend.run(&launch).await?;
    host.agent_started(&prepared).await;
    Ok((started, false))
}

/// What identifies the agent of a Task: its working place. The rest is for
/// the apps.
fn labels(p: &Prepared) -> Labels {
    let mut labels = Labels::from([
        (KIND.to_owned(), "task".to_owned()),
        (ROLE.to_owned(), "agent".to_owned()),
        (DIR.to_owned(), p.dir.to_string_lossy().into_owned()),
        (WORKSPACE.to_owned(), p.workspace.clone()),
        (TITLE.to_owned(), p.title.clone()),
    ]);
    if let Some(repo) = &p.repo {
        labels.insert(REPO.to_owned(), repo.to_string_lossy().into_owned());
    }
    if let Some(branch) = &p.branch {
        labels.insert(BRANCH.to_owned(), branch.clone());
    }
    if let Some(repo) = &p.checkout {
        labels.insert(CHECKOUT.to_owned(), repo.clone());
    }
    if let Some(bot) = &p.bot {
        labels.insert(BOT.to_owned(), bot.clone());
    }
    if let Some(name) = &p.subscription {
        labels.insert(SUBSCRIPTION.to_owned(), name.clone());
    }
    if let Ok(places) = serde_json::to_string(&p.places) {
        labels.insert(PLACES.to_owned(), places);
    }
    if !p.connections.is_empty()
        && let Ok(names) = serde_json::to_string(&p.connections)
    {
        labels.insert(CONNECTIONS.to_owned(), names);
    }
    if !p.mcp.is_empty()
        && let Ok(names) = serde_json::to_string(&p.mcp)
    {
        labels.insert(MCP.to_owned(), names);
    }
    labels
}

/// The identifying labels only: a running agent matches on these.
fn identity(labels: &Labels) -> impl Iterator<Item = (&String, &String)> {
    labels
        .iter()
        .filter(|(k, _)| [KIND, ROLE, DIR].contains(&k.as_str()))
}
