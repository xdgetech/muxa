//! Serves task records to clients (ADR 0015, 0018): `list-tasks`,
//! `create-task`, `update-task`, `restart-agent`, and the link between a
//! record and its session. The store itself is `records/`, which knows
//! nothing of tmux or the protocol.

pub(crate) mod branch_task;
pub(crate) mod brief;
pub(crate) mod close;
mod create;
pub(crate) mod end;
mod facts;
pub(crate) mod handoff;
pub(crate) mod observation;
pub(crate) mod park;
pub(crate) mod reading;
pub mod remote;
pub(crate) mod reply;
pub(crate) mod restart;
pub(crate) mod sweep;

pub use brief::{BRIEF, HANDOFF};
pub use restart::restart;

use std::cmp::Reverse;
use std::collections::HashSet;
use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::task::JoinHandle;

use muxa_protocol::{ErrorBody, ErrorCode, Message, Request, TaskStatus};
use muxa_tmux::{Client, Role, Session};

use crate::agents::Registry;
use crate::convert::{self, error_body};
use crate::records::{AgentSettings, Record, Status, Store, StoreError, Workspaces};
use crate::tasks::same_dir;

pub use create::{Env, NewTask, create};

/// The most records one `task-list` carries.
const MAX_LIST: usize = 200;

/// Where the records and the workspaces config live.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    /// None when neither `--state-dir` nor `HOME` gives one: no task records.
    pub state_dir: Option<PathBuf>,
    /// None means no workspaces.
    pub workspaces: Option<PathBuf>,
    /// Expands `~/` in `workspaces.toml`.
    pub home: Option<PathBuf>,
}

impl Config {
    pub fn enabled(&self) -> bool {
        self.state_dir.is_some()
    }

    /// Opens (and creates) the store. Blocking.
    pub(crate) fn store(&self) -> Result<Store, ErrorBody> {
        let dir = self.state_dir.as_ref().ok_or_else(|| {
            error_body(
                ErrorCode::Unsupported,
                "this bridge does not keep task records: HOME is not set; pass --state-dir",
            )
        })?;
        Store::open(dir).map_err(|e| store_error(&e))
    }

    /// The records, without creating the store. Blocking.
    pub(crate) fn records_if_any(&self) -> Vec<Record> {
        let Some(dir) = self.state_dir.as_ref().filter(|d| d.is_dir()) else {
            return Vec::new();
        };
        match Store::open(dir).and_then(|s| s.list()) {
            Ok(records) => records,
            Err(e) => {
                tracing::warn!(error = %e, "cannot read task records");
                Vec::new()
            }
        }
    }

    /// Read on every request, so editing the file needs no bridge restart. Blocking.
    pub(crate) fn load_workspaces(&self) -> Result<Workspaces, ErrorBody> {
        let Some(path) = &self.workspaces else {
            return Ok(Workspaces::default());
        };
        Workspaces::load(path, self.home.as_deref()).map_err(|e| {
            tracing::warn!(error = %e, "cannot load workspaces");
            error_body(
                ErrorCode::Internal,
                format!("{e}. Fix the workspaces file on the host."),
            )
        })
    }
}

/// Gives the tmux client that a launch or a restart needs. The bridge has
/// one for its whole session; the daemon attaches only while it has work.
pub trait ClientSource: Send + Sync {
    fn client(&self) -> ClientFuture<'_>;
}

pub type ClientFuture<'a> =
    Pin<Box<dyn Future<Output = Result<Arc<Client>, muxa_tmux::Error>> + Send + 'a>>;

/// The bridge's client, attached for the whole session.
pub struct Attached(pub Arc<Client>);

impl ClientSource for Attached {
    fn client(&self) -> ClientFuture<'_> {
        let client = Arc::clone(&self.0);
        Box::pin(async move { Ok(client) })
    }
}

/// What the record requests need, in the bridge and in the daemon alike.
#[derive(Clone)]
pub struct Context {
    pub tmux: Arc<dyn ClientSource>,
    pub tasks: Arc<crate::tasks::Config>,
    pub records: Arc<Config>,
    pub agents: Arc<Registry>,
    pub pending: Pending,
    /// This process is the host daemon: only it runs assistant requests (ADR 0046 §8).
    pub daemon: bool,
}

impl Context {
    /// The environment of a request that works in tmux.
    pub(crate) async fn env(&self) -> Result<Env, ErrorBody> {
        let client = self.tmux.client().await.map_err(|e| {
            tracing::warn!(error = %e, "cannot reach tmux for a task request");
            error_body(
                ErrorCode::Tmux,
                format!("cannot reach tmux on the host: {e}"),
            )
        })?;
        Ok(Env {
            client,
            tasks: Arc::clone(&self.tasks),
            records: Arc::clone(&self.records),
            agents: Arc::clone(&self.agents),
            pending: self.pending.clone(),
        })
    }
}

/// The requests that read or write task records. One code path serves them,
/// in the bridge and in the daemon (ADR 0019).
pub fn is_record_request(req: &Request) -> bool {
    matches!(
        req,
        Request::ListTasks { .. }
            | Request::CreateTask { .. }
            | Request::UpdateTask { .. }
            | Request::RestartAgent { .. }
            | Request::BranchTask { .. }
            | Request::ParkTask { .. }
            | Request::RecoverRestart { .. }
            | Request::StartLoop { .. }
            | Request::StopLoop { .. }
            | Request::ListRoutines { .. }
            | Request::RunRoutine { .. }
            | Request::ListInbox { .. }
            | Request::UpdateInbox { .. }
            | Request::Capture { .. }
            | Request::SortCapture { .. }
            | Request::UpdateWork { .. }
            | Request::RequestAssistant { .. }
            | Request::CancelRequest { .. }
            | Request::GetWork { .. }
            | Request::ApplyNotes { .. }
            | Request::PlanNow { .. }
    )
}

/// Answers one record request: its `task-list` or `task-saved`, then its `result`.
pub async fn handle(cx: &Context, req: Request) -> Vec<Message> {
    let id = req.id();
    let (mut out, error) = match req {
        Request::ListTasks { id } => one(list(&cx.records, id).await),
        Request::UpdateTask {
            id,
            task,
            rev,
            status,
            worktree: Some(choice),
        } if matches!(status, TaskStatus::Done | TaskStatus::Dropped) => {
            one(end::end(cx, &task, Some(rev), status, choice)
                .await
                .map(|record| Message::TaskSaved {
                    task: id,
                    record: convert::task_record(record),
                }))
        }
        Request::UpdateTask {
            id,
            task,
            rev,
            status,
            ..
        } => {
            let saved = update(&cx.records, id, task.clone(), rev, status).await;
            if saved.is_ok() {
                close::close_when_ended(cx, &task, status);
            }
            one(saved)
        }
        Request::CreateTask {
            id,
            workspace,
            title,
            goal,
            project,
            repo,
            branch,
            base,
            r#loop,
            worker,
            read,
            work,
            work_rev,
        } if !(read && r#loop.is_some()) => {
            let new = NewTask {
                worker,
                workspace,
                title,
                goal,
                project,
                repo,
                branch,
                base,
                read,
                routine: None,
                assistant: None,
                work: None,
            };
            let spec = r#loop.map(convert::loop_spec);
            let made = match (work, work_rev) {
                (None, None) => create_with_loop(cx, new, spec).await,
                (Some(work), Some(rev)) => {
                    crate::assistant::start::create_task(cx, new, spec, work, rev).await
                }
                _ => Err(error_body(
                    ErrorCode::BadRequest,
                    "create-task: work and workRev come together",
                )),
            };
            match made {
                Ok((record, error)) => (saved(id, record).into_iter().collect(), error),
                Err(e) => (Vec::new(), Some(e)),
            }
        }
        Request::StartLoop {
            id,
            task,
            r#loop,
            reset,
        } => {
            let (config, spec) = (Arc::clone(&cx.records), convert::loop_spec(r#loop));
            one(blocking(move || {
                crate::loops::start(&config, &task, spec, reset).map_err(|e| loop_error(&e))
            })
            .await
            .map(|r| task_saved(id, r)))
        }
        Request::StopLoop { id, task } => {
            let config = Arc::clone(&cx.records);
            one(
                blocking(move || crate::loops::stop(&config, &task).map_err(|e| loop_error(&e)))
                    .await
                    .map(|r| task_saved(id, r)),
            )
        }
        Request::RecoverRestart {
            id,
            task,
            rev,
            operation,
            action,
        } => match cx.env().await {
            Ok(env) => one(restart::recover(&env, task, rev, operation, action)
                .await
                .map(|r| task_saved(id, r))),
            Err(e) => (Vec::new(), Some(e)),
        },
        Request::CreateTask { .. } => (
            Vec::new(),
            Some(error_body(
                ErrorCode::BadRequest,
                "a reading task has no Task Loop; branch it first",
            )),
        ),
        Request::BranchTask { id, task, branch } => match cx.env().await {
            Ok(env) => one(branch_task::branch(&env, task, branch)
                .await
                .map(|r| task_saved(id, r))),
            Err(e) => (Vec::new(), Some(e)),
        },
        Request::ParkTask { id, task } => match cx.env().await {
            Ok(env) => one(park::park(&env, task).await.map(|r| task_saved(id, r))),
            Err(e) => (Vec::new(), Some(e)),
        },
        Request::RestartAgent {
            id,
            task,
            force,
            worker,
            rev,
        } => match cx.env().await {
            Ok(env) => match restart(&env, task, force, worker, rev).await {
                Ok(record) => (saved(id, Some(record)).into_iter().collect(), None),
                Err(e) => (Vec::new(), Some(e)),
            },
            Err(e) => (Vec::new(), Some(e)),
        },
        req @ (Request::ListRoutines { .. }
        | Request::RunRoutine { .. }
        | Request::ListInbox { .. }
        | Request::UpdateInbox { .. }) => crate::routines::handle(cx, req).await,
        req @ (Request::Capture { .. }
        | Request::SortCapture { .. }
        | Request::UpdateWork { .. }
        | Request::RequestAssistant { .. }
        | Request::CancelRequest { .. }
        | Request::GetWork { .. }
        | Request::ApplyNotes { .. }
        | Request::PlanNow { .. }) => crate::assistant::handle(cx, req).await,
        _ => (
            Vec::new(),
            Some(error_body(
                ErrorCode::Unsupported,
                "not a task record request",
            )),
        ),
    };
    out.push(Message::Result {
        id: Some(id),
        error,
        sessions: None,
    });
    out
}

/// Resumes the prompt waits that an ended process left in the records
/// (ADR 0019): a wait younger than [`handoff::PROMPT_WAIT`] goes on, an
/// older one ends with the note that the prompt was not sent.
pub async fn resume_prompts(cx: &Context) {
    let config = Arc::clone(&cx.records);
    let waits = blocking(move || {
        Ok(config
            .records_if_any()
            .into_iter()
            .filter_map(|r| {
                let pending = r.pending_prompt()?;
                handoff::supports_prompt(&r, pending).then_some((r.id.clone(), pending))
            })
            .collect::<Vec<_>>())
    })
    .await
    .unwrap_or_default();
    for (task, pending) in waits {
        let age = crate::records::now().saturating_sub(pending.since);
        let env = if age < handoff::PROMPT_WAIT {
            cx.env().await.ok()
        } else {
            None
        };
        let Some(env) = env else {
            tracing::info!(task, age, "a prompt wait ran out while no process waited");
            let note = handoff::Prompt::of(pending).not_sent;
            handoff::give_up(Arc::clone(&cx.records), task, pending, None, None, note).await;
            continue;
        };
        tracing::info!(task, age, "resuming a prompt wait");
        let wait = handoff::Wait {
            which: handoff::Linked::Task(task.clone()),
            task,
            pending,
        };
        env.pending.spawn(handoff::send_prompt(
            env.client,
            env.agents,
            env.records,
            wait,
        ));
    }
}

/// `create-task`, with its loop when it asks for one. The loop is checked
/// against the workspace first, so a refused loop leaves no Task behind.
pub(crate) async fn create_with_loop(
    cx: &Context,
    new: NewTask,
    spec: Option<crate::records::LoopSpec>,
) -> Result<(Option<Record>, Option<ErrorBody>), ErrorBody> {
    if let Some(spec) = &spec {
        let main = match &new.repo {
            Some(repo) => Some(
                crate::tasks::builtin::main_checkout(
                    &repo_path(&cx.records, &new.workspace, repo).await?,
                )
                .await
                .map_err(|e| convert::task_error(&e))?,
            ),
            None => None,
        };
        let (config, workspace, spec) =
            (Arc::clone(&cx.records), new.workspace.clone(), spec.clone());
        blocking(move || {
            crate::loops::precheck(&config, &workspace, main.as_deref(), &spec)
                .map_err(|e| loop_error(&e))
        })
        .await?;
    }
    let env = cx.env().await?;
    let (record, error) = create(&env, new).await;
    let (Some(record), None, Some(spec)) = (&record, &error, spec) else {
        return Ok((record, error));
    };
    let (config, task) = (Arc::clone(&cx.records), record.id.clone());
    match blocking(move || {
        crate::loops::start(&config, &task, spec, false).map_err(|e| loop_error(&e))
    })
    .await
    {
        Ok(r) => Ok((Some(r), None)),
        Err(e) => Ok((Some(record.clone()), Some(e))),
    }
}

/// `repo` as `create-task` names it, resolved in `workspace` (ADR 0031).
async fn repo_path(
    config: &Arc<Config>,
    workspace: &str,
    repo: &str,
) -> Result<PathBuf, ErrorBody> {
    let (config, workspace, repo) = (Arc::clone(config), workspace.to_owned(), repo.to_owned());
    blocking(move || {
        let workspaces = config.load_workspaces()?;
        let ws = workspaces.get(&workspace).ok_or_else(|| {
            error_body(
                ErrorCode::BadRequest,
                format!("no workspace named {workspace:?}; list-tasks shows the host's workspaces"),
            )
        })?;
        ws.repo_path(&repo)
            .map_err(|e| error_body(ErrorCode::BadRequest, e.to_string()))
    })
    .await
}

/// `start-session`: an ask session in a repo's main checkout (ADR 0031).
/// Not a record request: the bridge serves it with its own client.
pub async fn start_session(
    client: &Client,
    tasks: &crate::tasks::Config,
    config: &Config,
    workspace: &str,
    repo: &str,
) -> Result<crate::tasks::Launched, ErrorBody> {
    let loaded = config.clone();
    let workspaces = blocking(move || loaded.load_workspaces()).await?;
    crate::tasks::ask::start(client, &tasks.command, &workspaces, workspace, repo)
        .await
        .map_err(|e| {
            tracing::warn!(error = %e, "ask session did not start");
            convert::task_error(&e)
        })
}

fn task_saved(request: u64, record: Record) -> Message {
    Message::TaskSaved {
        task: request,
        record: convert::task_record(record),
    }
}

/// Loop messages name ids and fields, never the check command (invariant 5).
fn loop_error(e: &crate::loops::LoopError) -> ErrorBody {
    use crate::loops::LoopError;
    match e {
        LoopError::Bad(m) => error_body(ErrorCode::BadRequest, m.clone()),
        LoopError::Store(s) => store_error(s),
        LoopError::Config(c) => {
            tracing::warn!(error = %c, "cannot load workspaces for a loop");
            error_body(ErrorCode::Internal, e.to_string())
        }
    }
}

fn one(m: Result<Message, ErrorBody>) -> (Vec<Message>, Option<ErrorBody>) {
    match m {
        Ok(m) => (vec![m], None),
        Err(e) => (Vec::new(), Some(e)),
    }
}

fn saved(request: u64, record: Option<Record>) -> Option<Message> {
    record.map(|r| Message::TaskSaved {
        task: request,
        record: convert::task_record(r),
    })
}

/// Work a client started that must finish after it leaves: a launch, and
/// the wait to type the start prompt. Without it, a client that sends
/// `create-task` and closes its input (`muxa-cli task new`) would end the
/// bridge, and the prompt with it.
#[derive(Clone, Default)]
pub struct Pending(Arc<Mutex<Vec<JoinHandle<()>>>>);

impl Pending {
    pub fn spawn(&self, work: impl Future<Output = ()> + Send + 'static) {
        let handle = tokio::spawn(work);
        let mut all = self.0.lock().unwrap_or_else(|e| e.into_inner());
        all.retain(|h| !h.is_finished());
        all.push(handle);
    }

    /// Waits for every pending task, also those started meanwhile, until
    /// `limit`. Tasks still running then are aborted.
    pub async fn finish(&self, limit: Duration) {
        let deadline = tokio::time::Instant::now() + limit;
        loop {
            let handles = std::mem::take(&mut *self.0.lock().unwrap_or_else(|e| e.into_inner()));
            if handles.is_empty() {
                return;
            }
            for mut h in handles {
                if tokio::time::timeout_at(deadline, &mut h).await.is_err() {
                    tracing::warn!("a pending task did not finish; stopping it");
                    h.abort();
                }
            }
        }
    }
}

/// The agent settings of the one workspace whose `repos` lists the main
/// checkout of `repo`, for a `new-task` that names no workspace (ADR 0016).
/// None when no workspace or several list it.
pub async fn agent_for_repo(config: &Config, repo: &std::path::Path) -> Option<AgentSettings> {
    let main = crate::tasks::builtin::main_checkout(repo).await.ok()?;
    let config = config.clone();
    blocking(move || {
        let workspaces = config.load_workspaces()?;
        let listing = workspaces.listing(&main);
        if listing.len() > 1 {
            tracing::warn!(
                workspaces = listing.len(),
                "several workspaces list the repo; starting without a workspace environment"
            );
        }
        Ok(match listing.as_slice() {
            [one] => one.agent.clone(),
            _ => None,
        })
    })
    .await
    .ok()
    .flatten()
}

/// The `task-list` for a `list-tasks` request.
pub async fn list(config: &Config, request: u64) -> Result<Message, ErrorBody> {
    let config = config.clone();
    blocking(move || {
        let workspaces = config.load_workspaces()?;
        let mut records = config.store()?.list().map_err(|e| store_error(&e))?;
        // A Routine or assistant run is not a Task (ADR 0046 section 5).
        records.retain(|r| !r.is_background_run());
        let (records, truncated) = cap(records, MAX_LIST);
        Ok(Message::TaskList {
            tasks: request,
            workspaces: workspaces.all().iter().map(convert::workspace).collect(),
            entries: records.into_iter().map(convert::task_record).collect(),
            truncated,
            workers: workspaces.workers().iter().map(convert::worker).collect(),
        })
    })
    .await
}

/// Sets a record's status if the client saw its current `rev`.
pub async fn update(
    config: &Config,
    request: u64,
    task: String,
    rev: u64,
    status: TaskStatus,
) -> Result<Message, ErrorBody> {
    let config = config.clone();
    blocking(move || {
        let record = write_status(&config, &task, Some(rev), status)?;
        Ok(Message::TaskSaved {
            task: request,
            record: convert::task_record(record),
        })
    })
    .await
}

/// Sets a record's status at its current revision: an Inbox answer (ADR 0033).
pub(crate) async fn set_status(
    config: &Config,
    task: String,
    status: TaskStatus,
) -> Result<Record, ErrorBody> {
    let config = config.clone();
    blocking(move || write_status(&config, &task, None, status)).await
}

/// Blocking. `rev` None takes the record's current one.
fn write_status(
    config: &Config,
    task: &str,
    rev: Option<u64>,
    status: TaskStatus,
) -> Result<Record, ErrorBody> {
    let store = config.store()?;
    let rev = match rev {
        Some(rev) => rev,
        None => store.get(task).map_err(|e| store_error(&e))?.rev,
    };
    let record = store
        .update(task, rev, |r| r.status = status_of(status))
        .map_err(|e| store_error(&e))?;
    tracing::info!(task = %record.id, rev = record.rev, "task status set");
    if matches!(status, TaskStatus::Done | TaskStatus::Dropped) {
        crate::items::produce::task_closed(config, task);
    }
    Ok(record)
}

/// Fills `task_id` of sessions without a `@muxa_task` tag, from the records'
/// `dir`: a session whose worktree, or first visible pane's path, is a
/// record's `dir` runs it. A record whose tag some session carries is not
/// matched again. The tag is not written back: listing changes nothing in
/// tmux. Blocking (paths are canonicalized).
pub fn link(sessions: &mut [Session], config: &Config) {
    if !config.enabled() || sessions.iter().all(|s| s.task_id.is_some()) {
        return;
    }
    link_records(sessions, &config.records_if_any());
}

fn link_records(sessions: &mut [Session], records: &[Record]) {
    let mut taken: HashSet<String> = sessions.iter().filter_map(|s| s.task_id.clone()).collect();
    for s in sessions.iter_mut().filter(|s| s.task_id.is_none()) {
        let Some(dir) = session_dir(s) else { continue };
        let found = records.iter().find(|r| {
            !taken.contains(&r.id) && r.dir.as_deref().is_some_and(|d| same_dir(d, &dir))
        });
        if let Some(r) = found {
            taken.insert(r.id.clone());
            s.task_id = Some(r.id.clone());
        }
    }
}

fn session_dir(s: &Session) -> Option<PathBuf> {
    s.worktree.as_ref().map(PathBuf::from).or_else(|| {
        s.panes
            .iter()
            .find(|p| p.role != Role::Hidden)
            .filter(|p| !p.path.is_empty())
            .map(|p| PathBuf::from(&p.path))
    })
}

/// At most `max` records: every unfinished one first, then the newest.
/// Returned oldest first, as the store lists them.
fn cap(mut records: Vec<Record>, max: usize) -> (Vec<Record>, bool) {
    if records.len() <= max {
        return (records, false);
    }
    records.sort_by_key(|r| (finished(r), Reverse(r.created_at)));
    records.truncate(max);
    records.sort_by(|a, b| (a.created_at, &a.id).cmp(&(b.created_at, &b.id)));
    (records, true)
}

fn finished(r: &Record) -> bool {
    matches!(r.status, Status::Done | Status::Dropped)
}

fn status_of(s: TaskStatus) -> Status {
    match s {
        TaskStatus::Open => Status::Open,
        TaskStatus::Review => Status::Review,
        TaskStatus::Done => Status::Done,
        TaskStatus::Dropped => Status::Dropped,
    }
}

/// Store messages carry ids, paths and field names only (invariant 5).
fn store_error(e: &StoreError) -> ErrorBody {
    let code = match e {
        StoreError::Conflict { .. } | StoreError::Busy(_) => ErrorCode::Conflict,
        StoreError::NotFound(_) | StoreError::InvalidId | StoreError::Invalid { .. } => {
            ErrorCode::BadRequest
        }
        _ => {
            tracing::warn!(error = %e, "task store failed");
            ErrorCode::Internal
        }
    };
    error_body(code, e.to_string())
}

async fn blocking<T: Send + 'static>(
    f: impl FnOnce() -> Result<T, ErrorBody> + Send + 'static,
) -> Result<T, ErrorBody> {
    tokio::task::spawn_blocking(f)
        .await
        .unwrap_or_else(|_| Err(error_body(ErrorCode::Internal, "the task store stopped")))
}

#[cfg(test)]
mod tests;
