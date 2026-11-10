//! `create-task`: check the request, write the record, launch, tag the
//! session, write `TASK.md`, then type the start prompt in the background.

use std::sync::Arc;

use muxa_protocol::{ErrorBody, ErrorCode};
use muxa_tmux::Client;

use super::{Config, Pending, blocking, brief, handoff, store_error};
use crate::agents::Registry;
use crate::convert::{error_body, task_error};
use crate::records::lifecycle::ProcessIdentity;
use crate::records::store::ClaimedTask;
use crate::records::workers::{ResolvedWorker, WorkerChoice};
use crate::records::{NewRecord, PendingPrompt, PromptKind, Record, RunReason, Status, Workspace};
use crate::tasks::{self, Launched, TaskLauncher, TaskRequest, builtin};

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
    pub worker: Option<String>,
    pub title: String,
    pub goal: String,
    pub project: Option<String>,
    pub repo: Option<String>,
    pub branch: Option<String>,
    pub base: Option<String>,
    /// A reading Task (ADR 0034): `repo` and no `branch`.
    pub read: bool,
    /// Set when the daemon starts a Routine run (ADR 0026); never from a client.
    pub routine: Option<crate::routines::record::RunOf>,
    /// Set when the daemon starts an assistant run (ADR 0048); never from a client.
    pub assistant: Option<crate::assistant::record::RunFor>,
    /// The Work item the Task is for, checked by `assistant::start` (ADR 0048).
    pub work: Option<String>,
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
    /// A reading Task: its own folder, and the repo's main checkout to read.
    Read(Workspace, std::path::PathBuf),
}

/// The record to send in `task-saved`, if one was kept, and the request's
/// error. A failed launch keeps the record with a note (`task-failed`); a
/// wrong request keeps none (`bad-request`).
pub async fn create(env: &Env, new: NewTask) -> (Option<Record>, Option<ErrorBody>) {
    let (target, resolved) = match check(env, &new).await {
        Ok(t) => t,
        Err(e) => return (None, Some(e)),
    };
    let (repo, branch) = match &target {
        Target::Repo(req) => match builtin::main_checkout(&req.repo).await {
            Ok(main) => (Some(main), Some(req.branch.clone())),
            Err(e) => return (None, Some(task_error(&e))),
        },
        Target::Read(_, main) => (Some(main.clone()), None),
        Target::Dir(_) => (None, None),
    };
    let mut extra = match (&new.routine, &new.assistant) {
        (Some(r), _) => r.fields(),
        (None, Some(a)) => a.fields(),
        (None, None) => serde_json::Map::new(),
    };
    if new.read {
        extra.extend(crate::records::reading::new_fields());
    }
    if let Some(work) = &new.work {
        extra.insert(crate::records::WORK_FIELD.into(), work.clone().into());
    }
    let fields = NewRecord {
        workspace: new.workspace,
        worker: resolved.worker.clone(),
        title: new.title,
        goal: new.goal,
        project: new.project,
        repo,
        branch,
        extra,
        ..NewRecord::default()
    };
    let config = Arc::clone(&env.records);
    let record =
        match blocking(move || config.store()?.create(fields).map_err(|e| store_error(&e))).await {
            Ok(r) => r,
            Err(e) => return (None, Some(e)),
        };
    tracing::info!(task = %record.id, "task recorded");
    if record.worker().is_some() {
        return create_worker(env, record, &target, &resolved).await;
    }
    match start(env, &record, &target, &resolved).await {
        Ok(started) => linked(env, record, started.launched).await,
        Err(failure) => failed(env, record, failure).await,
    }
}

/// Checks the request at the boundary; nothing is written yet.
async fn check(env: &Env, new: &NewTask) -> Result<(Target, ResolvedWorker), ErrorBody> {
    let bad = |m: String| error_body(ErrorCode::BadRequest, m);
    let config = Arc::clone(&env.records);
    let workspace_name = new.workspace.clone();
    let selected = new.worker.clone();
    let fallback = env.tasks.command.clone();
    let (workspaces, resolved) = blocking(move || {
        let workspaces = config.load_workspaces()?;
        let choice = match selected.as_deref() {
            None | Some(muxa_protocol::DEFAULT_WORKER) => WorkerChoice::WorkspaceDefault,
            Some(name) => WorkerChoice::Named(name),
        };
        let resolved = workspaces
            .resolve_worker(&workspace_name, choice, None, &fallback)
            .map_err(|e| error_body(ErrorCode::BadRequest, e.to_string()))?;
        Ok((workspaces, resolved))
    })
    .await?;
    let workspace = workspaces.get(&new.workspace).cloned().ok_or_else(|| {
        bad(format!(
            "no workspace named {:?}; list-tasks shows the host's workspaces",
            new.workspace
        ))
    })?;
    let background = new.routine.is_some() || new.assistant.is_some();
    if background && (new.repo.is_some() || new.branch.is_some() || new.base.is_some()) {
        return Err(bad("a background run has no repo".into()));
    }
    if new.read {
        return read_target(new, workspace, &workspaces)
            .await
            .map(|t| (t, resolved));
    }
    let repo = match &new.repo {
        None if new.branch.is_none() && new.base.is_none() => {
            return Ok((Target::Dir(workspace), resolved));
        }
        None => return Err(bad("branch and base need a repo".into())),
        Some(r) => workspace
            .repo_path(r)
            .map_err(|e| bad(e.to_string()))?
            .to_string_lossy()
            .into_owned(),
    };
    let branch = match &new.branch {
        Some(b) => b.clone(),
        None => branch_from_title(&repo, &new.title).await?,
    };
    let branch = &branch;
    let mut req = tasks::validate(&repo, branch, new.base.as_deref())
        .await
        .map_err(|e| task_error(&e))?;
    let main = builtin::main_checkout(&req.repo)
        .await
        .map_err(|e| task_error(&e))?;
    if let Some(owner) = workspaces.owner_elsewhere(&main, &workspace.name) {
        return Err(bad(format!(
            "{} belongs to workspace {:?}; start the task there",
            main.display(),
            owner.name
        )));
    }
    req.agent = if resolved.worker.is_some() {
        Some(resolved.environment.clone())
    } else {
        workspace.agent.clone()
    };
    if resolved.worker.is_some() {
        if let Some(path) = builtin::existing_worktree(&main, branch)
            .await
            .map_err(|e| task_error(&e))?
        {
            if tasks::session_for_worktree(&env.client, &path)
                .await
                .map_err(|e| task_error(&e))?
                .is_some()
            {
                return Err(bad("a worker needs a new agent session; open the existing task or use another branch".into()));
            }
        }
    }
    if let Some(other) = branch_task_of(env, &req).await? {
        return Err(bad(format!(
            "{branch} already runs task {other}. Open that task, or use another branch."
        )));
    }
    Ok((Target::Repo(req), resolved))
}

/// A reading Task's target: a repo of this workspace, and no branch or base.
async fn read_target(
    new: &NewTask,
    workspace: Workspace,
    workspaces: &crate::records::Workspaces,
) -> Result<Target, ErrorBody> {
    let bad = |m: String| error_body(ErrorCode::BadRequest, m);
    let named_worker = new
        .worker
        .as_deref()
        .is_some_and(|w| w != muxa_protocol::DEFAULT_WORKER);
    if new.routine.is_some() || named_worker {
        return Err(bad(
            "a reading task runs the workspace's agent; leave out worker".into(),
        ));
    }
    let repo = new
        .repo
        .as_deref()
        .ok_or_else(|| bad("a reading task needs a repo".into()))?;
    if new.branch.is_some() || new.base.is_some() {
        return Err(bad(
            "a reading task has no branch or base; branch-task gives it one later".into(),
        ));
    }
    let path = workspace.repo_path(repo).map_err(|e| bad(e.to_string()))?;
    let main = builtin::main_checkout(&path)
        .await
        .map_err(|e| task_error(&e))?;
    if let Some(owner) = workspaces.owner_elsewhere(&main, &workspace.name) {
        return Err(bad(format!(
            "{} belongs to workspace {:?}; start the task there",
            main.display(),
            owner.name
        )));
    }
    Ok(Target::Read(workspace, main))
}

/// A new branch named after the title (ADR 0031). Its base is the repo's
/// default branch, never the main checkout's current one.
async fn branch_from_title(repo: &str, title: &str) -> Result<String, ErrorBody> {
    let path = std::path::Path::new(repo);
    if !path.is_dir() {
        return Err(error_body(
            ErrorCode::BadRequest,
            format!("{repo} is not a directory; give the repo's absolute path"),
        ));
    }
    let main = builtin::main_checkout(path)
        .await
        .map_err(|e| task_error(&e))?;
    tasks::branch::from_title(&main, title)
        .await
        .map_err(|e| task_error(&e))
}

/// The open task that the branch's session already runs, if any.
pub(super) async fn branch_task_of(
    env: &Env,
    req: &TaskRequest,
) -> Result<Option<String>, ErrorBody> {
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

struct Started {
    launched: Launched,
    initial: Option<(muxa_tmux::PaneId, ProcessIdentity)>,
}

/// Launches the agent, tags its session and writes `TASK.md`.
async fn start(
    env: &Env,
    record: &Record,
    target: &Target,
    resolved: &ResolvedWorker,
) -> Result<Started, Failure> {
    let client = &*env.client;
    let launched = match target {
        Target::Repo(req) => {
            let req = TaskRequest {
                record: Some(record.id.clone()),
                ..req.clone()
            };
            let launcher: Box<dyn TaskLauncher> = if resolved.worker.is_some() {
                Box::new(builtin::Builtin::with_worker(resolved))
            } else {
                tasks::pick_for(&env.tasks, client, &req).await
            };
            let result = launcher.launch(client, &req).await;
            if let Err(e) = &result {
                tracing::warn!(task = %record.id, launcher = launcher.id(), error = %e, "task launch failed");
            }
            result
        }
        Target::Read(workspace, main) => {
            let dir = workspace
                .task_dir(&record.title, &record.id)
                .map_err(Failure::before)?;
            let claude = super::reading::is_claude_code(&resolved.command);
            let (at, checkout) = (dir.clone(), main.clone());
            blocking(move || {
                std::fs::create_dir_all(&at).map_err(|e| {
                    error_body(ErrorCode::Internal, format!("cannot create {}: {e}", at.display()))
                })?;
                if claude {
                    super::reading::write_settings(&at, &checkout)
                        .map_err(|m| error_body(ErrorCode::Internal, m))?;
                }
                Ok(())
            })
            .await
            .map_err(|e| Failure::before(e.message))?;
            builtin::Builtin::new(env.tasks.command.clone())
                .launch_in_dir(client, &dir, workspace.agent.as_ref())
                .await
        }
        Target::Dir(workspace) => {
            let dir = match record.routine_run() {
                Some(run) => {
                    let dir = workspace
                        .routine_dir(&run.name, &record.id)
                        .map_err(Failure::before)?;
                    let (config, saved, at) =
                        (Arc::clone(&env.records), record.clone(), dir.clone());
                    blocking(move || {
                        crate::routines::prepare(&config, &saved, &at)
                            .map_err(|m| error_body(ErrorCode::Internal, m))
                    })
                    .await
                    .map_err(|e| Failure::before(e.message))?;
                    dir
                }
                None if record.is_assistant_run() => {
                    let dir = workspace
                        .assistant_dir(&record.title, &record.id)
                        .map_err(Failure::before)?;
                    let (at, root, saved) = (dir.clone(), workspace.root.clone(), record.clone());
                    let config = Arc::clone(&env.records);
                    blocking(move || {
                        crate::assistant::record::prepare_run(&config, &saved, &root, &at)
                            .map_err(|m| error_body(ErrorCode::Internal, m))
                    })
                    .await
                    .map_err(|e| Failure::before(e.message))?;
                    dir
                }
                None => workspace
                    .task_dir(&record.title, &record.id)
                    .map_err(Failure::before)?,
            };
            if resolved.worker.is_some() {
                builtin::Builtin::with_worker(resolved)
                    .launch_in_dir(client, &dir, Some(&resolved.environment)).await
            } else {
                builtin::Builtin::new(env.tasks.command.clone())
                    .launch_in_dir(client, &dir, workspace.agent.as_ref()).await
            }
        }
    }
    .map_err(Failure::before)?;
    let initial = if resolved.worker.is_some() {
        Some(
            startup_identity(env, &launched)
                .await
                .map_err(|error| Failure::after(&launched, error.message))?,
        )
    } else {
        None
    };
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
    let in_repo = record.repo.is_some() && !record.is_read();
    let (dir, saved) = (launched.dir.clone(), record.clone());
    let instructions = resolved.instructions.clone();
    blocking(move || {
        let result = if saved.worker().is_some() {
            brief::create_with_worker(&dir, &saved, in_repo, instructions.as_deref())
        } else {
            brief::create(&dir, &saved, in_repo)
        };
        result.map_err(|m| error_body(ErrorCode::Internal, m))
    })
    .await
    .map_err(|e| Failure::after(&launched, e.message))?;
    tracing::info!(task = %record.id, session = %launched.id, dir = %launched.dir.display(), "task linked");
    Ok(Started { launched, initial })
}

/// The `@muxa_task` the launched session carries already.
async fn tag_of(env: &Env, launched: &Launched) -> Result<Option<String>, muxa_tmux::Error> {
    let sessions = muxa_tmux::list_sessions(&env.client).await?;
    Ok(sessions
        .into_iter()
        .find(|s| s.id == launched.id)
        .and_then(|s| s.task_id))
}

/// Stores where the agent works and its first run, and starts waiting to
/// type the prompt.
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
                if r.is_read() {
                    r.set_folder(&dir);
                }
                r.dir = Some(dir);
                r.session = Some(name);
                let now = crate::records::now();
                let run = r.start_run(RunReason::Start, now);
                if let Some(current) = r.run_mut(run) {
                    current.set_worker(None);
                }
                crate::routines::record::arm(r, now);
                crate::assistant::record::arm(r, now);
                r.set_pending_prompt(Some(PendingPrompt {
                    kind: PromptKind::Start,
                    since: now,
                    run,
                }));
            })
            .map_err(|e| store_error(&e))
    })
    .await;
    let record = match saved {
        Ok(r) => r,
        Err(e) => return (Some(record), Some(e)),
    };
    spawn_prompt(env, &record, launched.id);
    (Some(record), None)
}

fn spawn_prompt(env: &Env, record: &Record, session: muxa_tmux::SessionId) {
    if let Some(pending) = record.pending_prompt() {
        env.pending.spawn(handoff::send_prompt(
            Arc::clone(&env.client),
            Arc::clone(&env.agents),
            Arc::clone(&env.records),
            handoff::Wait {
                task: record.id.clone(),
                which: handoff::Linked::Session(session),
                pending,
            },
        ));
    }
}

/// Hold the lifecycle claim from the first external effect through the startup
/// record. Failure bookkeeping uses this guard rather than reacquiring it.
async fn create_worker(
    env: &Env,
    record: Record,
    target: &Target,
    resolved: &ResolvedWorker,
) -> (Option<Record>, Option<ErrorBody>) {
    let config = Arc::clone(&env.records);
    let (id, rev) = (record.id.clone(), record.rev);
    let claim = blocking(move || {
        config
            .store()?
            .claim(&id, rev)
            .map_err(|error| store_error(&error))
    })
    .await;
    let claim = match claim {
        Ok(claim) => claim,
        Err(error) => return (Some(record), Some(error)),
    };
    let started = match start(env, &record, target, resolved).await {
        Ok(started) => started,
        Err(failure) => return failed_worker(Arc::clone(&env.records), claim, failure).await,
    };
    let Some((pane, initial)) = started.initial else {
        return failed_worker(
            Arc::clone(&env.records),
            claim,
            Failure::after(&started.launched, "worker startup identity is missing"),
        )
        .await;
    };
    let current = super::restart::identity(&env.client, started.launched.id, pane).await;
    if !current.is_ok_and(|process| process == initial) {
        return failed_worker(
            Arc::clone(&env.records),
            claim,
            Failure::after(
                &started.launched,
                "the worker process changed before startup was recorded",
            ),
        )
        .await;
    }
    let original = claim.record().clone();
    let (dir, name, session) = (
        started.launched.dir.clone(),
        started.launched.name.clone(),
        started.launched.id,
    );
    let operation = format!("start-{}", original.id);
    let saved = blocking(move || {
        let mut claim = claim;
        claim
            .update(|record| {
                record.dir = Some(dir);
                record.session = Some(name);
                let now = crate::records::now();
                let run = record.start_run(RunReason::Start, now);
                let worker = record.worker().map(str::to_owned);
                if let Some(current) = record.run_mut(run) {
                    current.set_worker(worker.as_deref());
                    crate::records::observation::bind_start(
                        current,
                        &operation,
                        session.number(),
                        pane.number(),
                        initial,
                    );
                }
                record.set_pending_prompt(Some(PendingPrompt {
                    kind: PromptKind::Start,
                    since: now,
                    run,
                }));
                crate::routines::record::arm(record, now);
                crate::assistant::record::arm(record, now);
            })
            .map_err(|error| store_error(&error))?;
        Ok(claim.record().clone())
    })
    .await;
    match saved {
        Ok(record) => {
            spawn_prompt(env, &record, session);
            (Some(record), None)
        }
        Err(error) => (Some(original), Some(error)),
    }
}

async fn startup_identity(
    env: &Env,
    launched: &Launched,
) -> Result<(muxa_tmux::PaneId, ProcessIdentity), ErrorBody> {
    let sessions = muxa_tmux::list_sessions(&env.client)
        .await
        .map_err(|error| {
            error_body(
                ErrorCode::TaskFailed,
                format!("cannot verify the started worker: {error}"),
            )
        })?;
    let pane = sessions
        .iter()
        .find(|session| session.id == launched.id)
        .and_then(|session| {
            session
                .panes
                .iter()
                .find(|pane| pane.role == muxa_tmux::Role::Agent && !pane.dead)
        })
        .ok_or_else(|| {
            error_body(
                ErrorCode::TaskFailed,
                "the started worker process is missing",
            )
        })?;
    let process = super::restart::identity(&env.client, launched.id, pane.id).await?;
    if process.pid != pane.pid {
        return Err(error_body(
            ErrorCode::Conflict,
            "the worker process changed during launch",
        ));
    }
    Ok((pane.id, process))
}

async fn failed_worker(
    config: Arc<Config>,
    mut claim: ClaimedTask,
    failure: Failure,
) -> (Option<Record>, Option<ErrorBody>) {
    let original = claim.record().clone();
    let error = error_body(ErrorCode::TaskFailed, failure.why.clone());
    let note = format!("The task did not start: {}", failure.why);
    let launched = failure.launched;
    let saved = blocking(move || {
        claim
            .update(|record| {
                record.note = Some(note.clone());
                if let Some(launched) = launched {
                    record.dir = Some(launched.dir);
                    record.session = Some(launched.name);
                }
            })
            .map_err(|error| store_error(&error))?;
        crate::items::produce::start_failed(&config, &claim.record().id, None, None, &note);
        Ok(claim.record().clone())
    })
    .await;
    (Some(saved.unwrap_or(original)), Some(error))
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
        let saved = config
            .store()?
            .update(&id, rev, |r| {
                r.note = Some(note.clone());
                if let Some(l) = launched {
                    r.dir = Some(l.dir);
                    r.session = Some(l.name);
                }
            })
            .map_err(|e| store_error(&e))?;
        crate::items::produce::start_failed(&config, &id, None, None, &note);
        Ok(saved)
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

#[cfg(all(test, feature = "tmux-integration"))]
mod worker_tests;
