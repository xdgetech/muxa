//! Closing a Task's session: on `close-task`, and when the Task ends (ADR 0037).
//! `close-task` also closes an ask session (ADR 0039).

use muxa_protocol::{CloseMode, ErrorBody, ErrorCode, TaskStatus};
use muxa_tmux::Client;

use super::{Config, Context};
use crate::convert::{self, error_body};
use crate::tasks;

pub(crate) async fn close_task(
    client: &Client,
    config: &tasks::Config,
    records: &Config,
    session: &str,
    mode: CloseMode,
) -> Result<(), ErrorBody> {
    let listed = muxa_tmux::list_sessions(client)
        .await
        .map_err(|e| convert::error(&e))?;
    let id = session.parse::<muxa_tmux::SessionId>().ok();
    if let Some(ask) = listed
        .iter()
        .find(|s| Some(s.id) == id && tasks::ask::is_ask(s))
    {
        return tasks::ask::close(client, ask)
            .await
            .map_err(|e| convert::task_error(&e));
    }
    let task = tasks::find_task(client, session)
        .await
        .map_err(|e| convert::task_error(&e))?;
    let tag = listed
        .into_iter()
        .find(|s| s.id == task.id)
        .and_then(|s| s.task_id);
    let launcher = tasks::pick(config, client).await;
    close_session(client, launcher.as_ref(), records, &task, tag, mode).await
}

/// Closes `session`, which `@muxa_task` links to `task`, keeping its files.
/// A session without `@agent_worktree` (a Task without a repo, a Routine run)
/// is not a launcher's task: the built-in launcher closes it at the record's `dir`.
pub(crate) async fn close_linked(
    client: &Client,
    config: &tasks::Config,
    records: &Config,
    session: &muxa_tmux::Session,
    task: &str,
) -> Result<(), ErrorBody> {
    if session.worktree.is_some() {
        let id = session.id.to_string();
        return close_task(client, config, records, &id, CloseMode::Keep).await;
    }
    let store = records.store()?;
    let dir = store
        .get(task)
        .map_err(|e| super::store_error(&e))?
        .dir
        .ok_or_else(|| error_body(ErrorCode::Conflict, "the Task has no folder"))?;
    let target = tasks::TaskSession {
        id: session.id,
        name: session.name.clone(),
        worktree: dir,
        repo_dir: None,
    };
    let builtin = tasks::builtin::Builtin::new(config.command.clone());
    close_session(
        client,
        &builtin,
        records,
        &target,
        Some(task.to_owned()),
        CloseMode::Keep,
    )
    .await
}

async fn close_session(
    client: &Client,
    launcher: &dyn tasks::TaskLauncher,
    records: &Config,
    task: &tasks::TaskSession,
    tag: Option<String>,
    mode: CloseMode,
) -> Result<(), ErrorBody> {
    let claim = claim_close(records.clone(), task.worktree.clone(), tag).await?;
    let result = launcher.close(client, task, mode).await;
    drop(claim);
    match &result {
        Ok(()) => {
            tracing::info!(launcher = launcher.id(), session = %task.id, ?mode, "task closed")
        }
        Err(e) => {
            tracing::warn!(launcher = launcher.id(), session = %task.id, error = %e, "closing failed")
        }
    }
    result.map_err(|e| convert::task_error(&e))
}

/// Session deletion participates in the same Task guard as restart and typing.
async fn claim_close(
    config: Config,
    worktree: std::path::PathBuf,
    task: Option<String>,
) -> Result<Option<crate::records::store::ClaimedTask>, ErrorBody> {
    tokio::task::spawn_blocking(move || {
        let fail = |error: crate::records::StoreError| {
            let code = match error {
                crate::records::StoreError::Busy(_)
                | crate::records::StoreError::Conflict { .. } => ErrorCode::Conflict,
                _ => ErrorCode::Internal,
            };
            error_body(code, error.to_string())
        };
        let Some(dir) = config.state_dir else {
            return if task.is_some() {
                Err(error_body(
                    ErrorCode::Conflict,
                    "cannot close a linked Task without its record store",
                ))
            } else {
                Ok(None)
            };
        };
        if task.is_none() && !dir.exists() {
            return Ok(None);
        }
        let store = crate::records::Store::open(dir).map_err(fail)?;
        let record = if let Some(task) = task {
            Some(store.get(&task).map_err(fail)?)
        } else {
            let mut matching = store.list().map_err(fail)?.into_iter().filter(|r| {
                r.dir
                    .as_deref()
                    .is_some_and(|d| tasks::same_dir(d, &worktree))
            });
            let first = matching.next();
            if matching.next().is_some() {
                return Err(error_body(
                    ErrorCode::Conflict,
                    "several Task records refer to this session",
                ));
            }
            first
        };
        let Some(record) = record else {
            return Ok(None);
        };
        let claim = store.claim(&record.id, record.rev).map_err(fail)?;
        if claim
            .record()
            .extra
            .contains_key(crate::records::lifecycle::FIELD)
        {
            return Err(error_body(
                ErrorCode::Conflict,
                "the Task restart requires recovery before closing its session",
            ));
        }
        if claim
            .record()
            .dir
            .as_deref()
            .is_some_and(|d| !tasks::same_dir(d, &worktree))
        {
            return Err(error_body(
                ErrorCode::Conflict,
                "the Task session directory changed",
            ));
        }
        Ok(Some(claim))
    })
    .await
    .map_err(|_| error_body(ErrorCode::Internal, "the Task close claim stopped"))?
}

/// After a Task became done or dropped: closes the session tagged with it,
/// keeping its worktree or folder (ADR 0037). A failure is logged; the
/// status stands.
pub(crate) async fn close_ended(cx: &Context, task: &str) {
    let env = match cx.env().await {
        Ok(env) => env,
        Err(e) => {
            tracing::warn!(task, error = %e.message, "cannot reach tmux to close an ended task");
            return;
        }
    };
    let sessions = match muxa_tmux::list_sessions(&env.client).await {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!(task, error = %e, "cannot list sessions to close an ended task");
            return;
        }
    };
    let Some(session) = sessions.iter().find(|s| s.task_id.as_deref() == Some(task)) else {
        return;
    };
    let id = session.id.to_string();
    match close_linked(&env.client, &cx.tasks, &cx.records, session, task).await {
        Ok(()) => tracing::info!(task, session = %id, "closed the ended task's session"),
        Err(e) => {
            tracing::warn!(task, session = %id, error = %e.message, "cannot close the ended task's session")
        }
    }
}

/// Starts [`close_ended`] in the background when `status` ends the Task.
pub(crate) fn close_when_ended(cx: &Context, task: &str, status: TaskStatus) {
    if !matches!(status, TaskStatus::Done | TaskStatus::Dropped) {
        return;
    }
    let (cx2, task) = (cx.clone(), task.to_owned());
    cx.pending
        .spawn(async move { close_ended(&cx2, &task).await });
}
