//! `create-task`: check the request, write the record, launch, tag the
//! session, write `TASK.md`, then type the start prompt in the background.

use std::sync::Arc;

use muxa_protocol::{ErrorBody, ErrorCode};
use muxa_tmux::Client;

use super::{Config, Pending, blocking, handoff, store_error};
use crate::agents::Registry;
use crate::convert::{error_body, task_error};
use crate::records::{NewRecord, Record, Status, Workspace};
use crate::tasks::{self, Launched, TaskRequest, builtin};

/// The session option that links a session to its record.
const TAG: &str = "@muxa_task";

/// What a `create-task` needs, shared with the prompt task that outlives it.
#[derive(Clone)]
pub struct Env {
    pub client: Arc<Client>,
    pub tasks: Arc<tasks::Config>,
    pub records: Arc<Config>,
    pub agents: Arc<Registry>,
    pub pending: Pending,
}

/// A `create-task` request as the client sent it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NewTask {
    pub workspace: String,
    pub title: String,
    pub goal: String,
    pub project: Option<String>,
    pub repo: Option<String>,
    pub branch: Option<String>,
    pub base: Option<String>,
}

/// Why a task did not start, and the session if one was made anyway.
struct Failure {
    launched: Option<Launched>,
    why: String,
}

impl Failure {
    fn before(why: impl ToString) -> Self {
        Self {
            launched: None,
            why: why.to_string(),
        }
    }

    fn after(launched: &Launched, why: impl ToString) -> Self {
        Self {
            launched: Some(launched.clone()),
            why: why.to_string(),
        }
    }
}

/// Where the agent of a new task works.
enum Target {
    Repo(TaskRequest),
    Dir(Workspace),
}

/// The record to send in `task-saved`, if one was kept, and the request's
/// error. A failed launch keeps the record with a note (`task-failed`); a
/// wrong request keeps none (`bad-request`).
pub async fn create(env: &Env, new: NewTask) -> (Option<Record>, Option<ErrorBody>) {
    let target = match check(env, &new).await {
        Ok(t) => t,
        Err(e) => return (None, Some(e)),
    };
    let repo = match &target {
        Target::Repo(req) => match builtin::main_checkout(&req.repo).await {
            Ok(main) => Some(main),
            Err(e) => return (None, Some(task_error(&e))),
        },
        Target::Dir(_) => None,
    };
    let fields = NewRecord {
        workspace: new.workspace,
        title: new.title,
        goal: new.goal,
        project: new.project,
        repo,
        branch: new.branch,
        ..NewRecord::default()
    };
    let config = Arc::clone(&env.records);
    let record =
        match blocking(move || config.store()?.create(fields).map_err(|e| store_error(&e))).await {
            Ok(r) => r,
            Err(e) => return (None, Some(e)),
        };
    tracing::info!(task = %record.id, "task recorded");
    match start(env, &record, &target).await {
        Ok(launched) => linked(env, record, launched).await,
        Err(failure) => failed(env, record, failure).await,
    }
}

/// Checks the request at the boundary; nothing is written yet.
async fn check(env: &Env, new: &NewTask) -> Result<Target, ErrorBody> {
    let bad = |m: String| error_body(ErrorCode::BadRequest, m);
    let config = Arc::clone(&env.records);
    let workspaces = blocking(move || config.load_workspaces()).await?;
    let workspace = workspaces.get(&new.workspace).cloned().ok_or_else(|| {
        bad(format!(
            "no workspace named {:?}; list-tasks shows the host's workspaces",
            new.workspace
        ))
    })?;
    let (repo, branch) = match (&new.repo, &new.branch) {
        (None, None) if new.base.is_none() => return Ok(Target::Dir(workspace)),
        (None, _) => return Err(bad("branch and base need a repo".into())),
        (Some(_), None) => return Err(bad("a task with a repo needs a branch".into())),
        (Some(r), Some(b)) => (r, b),
    };
    let req = tasks::validate(repo, branch, new.base.as_deref())
        .await
        .map_err(|e| task_error(&e))?;
    if let Some(other) = branch_task(env, &req).await? {
        return Err(bad(format!(
            "{branch} already runs task {other}. Open that task, or use another branch."
        )));
    }
    Ok(Target::Repo(req))
}

/// The open task that the branch's session already runs, if any.
async fn branch_task(env: &Env, req: &TaskRequest) -> Result<Option<String>, ErrorBody> {
    let main = builtin::main_checkout(&req.repo)
        .await
        .map_err(|e| task_error(&e))?;
    let Some(worktree) = builtin::existing_worktree(&main, &req.branch)
        .await
        .map_err(|e| task_error(&e))?
    else {
        return Ok(None);
    };
    let session = tasks::session_for_worktree(&env.client, &worktree)
        .await
        .map_err(|e| task_error(&e))?;
    match session.and_then(|s| s.task_id) {
        Some(id) => Ok(open_task(env, id).await),
        None => Ok(None),
    }
}

/// `id` when it names a record that is still open or in review.
async fn open_task(env: &Env, id: String) -> Option<String> {
    let config = Arc::clone(&env.records);
    blocking(move || {
        let r = config.store()?.get(&id).map_err(|e| store_error(&e))?;
        Ok(matches!(r.status, Status::Open | Status::Review).then_some(id))
    })
    .await
    .ok()
    .flatten()
}

/// Launches the agent, tags its session and writes `TASK.md`.
async fn start(env: &Env, record: &Record, target: &Target) -> Result<Launched, Failure> {
    let client = &*env.client;
    let launched = match target {
        Target::Repo(req) => {
            let launcher = tasks::pick(&env.tasks, client).await;
            let req = TaskRequest {
                record: Some(record.id.clone()),
                ..req.clone()
            };
            let result = launcher.launch(client, &req).await;
            if let Err(e) = &result {
                tracing::warn!(task = %record.id, launcher = launcher.id(), error = %e, "task launch failed");
            }
            result
        }
        Target::Dir(workspace) => {
            let dir = workspace
                .task_dir(&record.title, &record.id)
                .map_err(Failure::before)?;
            let launcher = builtin::Builtin::new(env.tasks.command.clone());
            launcher.launch_in_dir(client, &dir).await
        }
    }
    .map_err(Failure::before)?;
    let tagged = tag_of(env, &launched)
        .await
        .map_err(|e| Failure::after(&launched, e))?;
    if let Some(other) = tagged.filter(|t| *t != record.id) {
        if open_task(env, other.clone()).await.is_some() {
            // Not linked: the session belongs to the other task.
            return Err(Failure::before(format!(
                "{} already runs task {other}",
                launched.name
            )));
        }
    }
    muxa_tmux::set_session_option(client, launched.id, TAG, &record.id)
        .await
        .map_err(|e| Failure::after(&launched, e))?;
    let (dir, saved, in_repo) = (launched.dir.clone(), record.clone(), record.repo.is_some());
    blocking(move || {
        handoff::write(&dir, &saved, in_repo).map_err(|m| error_body(ErrorCode::Internal, m))
    })
    .await
    .map_err(|e| Failure::after(&launched, e.message))?;
    tracing::info!(task = %record.id, session = %launched.id, dir = %launched.dir.display(), "task linked");
    Ok(launched)
}

/// The `@muxa_task` the launched session carries already.
async fn tag_of(env: &Env, launched: &Launched) -> Result<Option<String>, muxa_tmux::Error> {
    let sessions = muxa_tmux::list_sessions(&env.client).await?;
    Ok(sessions
        .into_iter()
        .find(|s| s.id == launched.id)
        .and_then(|s| s.task_id))
}

/// Stores where the agent works, and starts waiting to type the prompt.
async fn linked(
    env: &Env,
    record: Record,
    launched: Launched,
) -> (Option<Record>, Option<ErrorBody>) {
    let config = Arc::clone(&env.records);
    let (id, rev) = (record.id.clone(), record.rev);
    let (dir, name) = (launched.dir.clone(), launched.name.clone());
    let saved = blocking(move || {
        config
            .store()?
            .update(&id, rev, |r| {
                r.dir = Some(dir);
                r.session = Some(name);
            })
            .map_err(|e| store_error(&e))
    })
    .await;
    let record = match saved {
        Ok(r) => r,
        Err(e) => return (Some(record), Some(e)),
    };
    env.pending.spawn(handoff::send_prompt(
        Arc::clone(&env.client),
        Arc::clone(&env.agents),
        Arc::clone(&env.records),
        record.id.clone(),
        launched.id,
    ));
    (Some(record), None)
}

/// Keeps the record with why the task did not start. It names a session
/// only when one was made before the failure.
async fn failed(
    env: &Env,
    record: Record,
    failure: Failure,
) -> (Option<Record>, Option<ErrorBody>) {
    let config = Arc::clone(&env.records);
    let (id, rev) = (record.id.clone(), record.rev);
    let note = format!("The task did not start: {}", failure.why);
    let launched = failure.launched;
    let saved = blocking(move || {
        config
            .store()?
            .update(&id, rev, |r| {
                r.note = Some(note);
                if let Some(l) = launched {
                    r.dir = Some(l.dir);
                    r.session = Some(l.name);
                }
            })
            .map_err(|e| store_error(&e))
    })
    .await;
    let record = match saved {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!(task = %record.id, error = %e.message, "cannot note a failed launch");
            record
        }
    };
    (
        Some(record),
        Some(error_body(ErrorCode::TaskFailed, failure.why)),
    )
}
