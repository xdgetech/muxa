//! Serves task records to clients (ADR 0015, 0018): `list-tasks`,
//! `create-task`, `update-task`, `restart-agent`, and the link between a
//! record and its session. The store itself is `records/`, which knows
//! nothing of tmux or the protocol.

mod brief;
mod create;
mod facts;
mod handoff;
mod restart;

pub use brief::BRIEF;
pub use restart::restart;

use std::cmp::Reverse;
use std::collections::HashSet;
use std::future::Future;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::task::JoinHandle;

use muxa_protocol::{ErrorBody, ErrorCode, Message, TaskStatus, WorkspaceInfo};
use muxa_tmux::{Role, Session};

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
    fn store(&self) -> Result<Store, ErrorBody> {
        let dir = self.state_dir.as_ref().ok_or_else(|| {
            error_body(
                ErrorCode::Unsupported,
                "this bridge does not keep task records: HOME is not set; pass --state-dir",
            )
        })?;
        Store::open(dir).map_err(|e| store_error(&e))
    }

    /// The records, without creating the store. Blocking.
    fn records_if_any(&self) -> Vec<Record> {
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
    fn load_workspaces(&self) -> Result<Workspaces, ErrorBody> {
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
        let records = config.store()?.list().map_err(|e| store_error(&e))?;
        let (records, truncated) = cap(records, MAX_LIST);
        Ok(Message::TaskList {
            tasks: request,
            workspaces: workspaces
                .all()
                .iter()
                .map(|w| WorkspaceInfo {
                    name: w.name.clone(),
                })
                .collect(),
            entries: records.into_iter().map(convert::task_record).collect(),
            truncated,
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
        let record = config
            .store()?
            .update(&task, rev, |r| r.status = status_of(status))
            .map_err(|e| store_error(&e))?;
        tracing::info!(task = %record.id, rev = record.rev, "task status set");
        Ok(Message::TaskSaved {
            task: request,
            record: convert::task_record(record),
        })
    })
    .await
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
        StoreError::Conflict { .. } => ErrorCode::Conflict,
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
