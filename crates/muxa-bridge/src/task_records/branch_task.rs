//! `branch-task` (ADR 0034 §3): a reading Task gets a worktree, then its
//! agent restarts there through the restart path of ADR 0018. The Task's
//! folder stays with what it wrote; changes in the main checkout stay too.
use std::path::PathBuf;
use std::sync::Arc;

use muxa_protocol::{ErrorBody, ErrorCode};
use muxa_tmux::AgentState;

use super::create::Env;
use super::handoff::{self, Linked};
use super::{blocking, brief, restart, store_error};
use crate::convert::{error_body, task_error};
use crate::records::{Record, Status};
use crate::tasks::{self, TaskRequest, builtin};

pub async fn branch(env: &Env, task: String, branch: Option<String>) -> Result<Record, ErrorBody> {
    let config = Arc::clone(&env.records);
    let id = task.clone();
    let record = blocking(move || config.store()?.get(&id).map_err(|e| store_error(&e))).await?;
    let (main, folder) = reading_parts(&record)?;
    let branch = match branch {
        Some(b) => b,
        None => tasks::branch::from_title(&main, &record.title)
            .await
            .map_err(|e| task_error(&e))?,
    };
    let req = tasks::validate(&main.to_string_lossy(), &branch, None)
        .await
        .map_err(|e| task_error(&e))?;
    if let Some(other) = super::create::branch_task_of(env, &req).await? {
        return Err(bad(format!(
            "{branch} already runs task {other}. Use another branch."
        )));
    }
    // The restart below refuses a running agent; nothing is made before that check.
    let pane = handoff::agent_pane(
        &env.client,
        &env.agents,
        &env.records,
        Linked::Task(record.id.clone()),
    )
    .await
    .ok_or_else(|| {
        error_body(
            ErrorCode::TaskFailed,
            "no session runs this task with an agent pane; resume it first",
        )
    })?;
    if pane.status.is_some_and(|s| s.state == AgentState::Running) {
        return Err(bad("the agent is running; branch once its turn ends"));
    }
    let branched = make_worktree(env, &record, &req, &folder).await?;
    tracing::info!(task = %branched.id, dir = ?branched.dir, "reading task branched");
    tag_worktree(
        &env.client,
        pane.session,
        &main,
        &work_dir(&branched)?,
        &branch,
        &branched.id,
    )
    .await;
    match restart::restart_record(env, branched.clone(), false, None, None).await {
        Ok(record) => Ok(record),
        Err(e) => {
            let note = format!(
                "Branch made the worktree, but the agent did not restart there: {}. Restart the agent.",
                e.message
            );
            handoff::write(&env.records, branched.id.clone(), move |r| {
                r.note = Some(note.clone());
            })
            .await;
            Err(e)
        }
    }
}

/// The main checkout and the folder of a reading Task that is still open.
fn reading_parts(record: &Record) -> Result<(PathBuf, PathBuf), ErrorBody> {
    if !record.is_read() {
        return Err(bad(format!(
            "task {} is not a reading task; it has its directory already",
            record.id
        )));
    }
    if !matches!(record.status, Status::Open | Status::Review) {
        return Err(bad(format!("task {} is closed", record.id)));
    }
    let main = record
        .repo
        .clone()
        .ok_or_else(|| bad("the reading task has no repo"))?;
    let folder = record
        .dir
        .clone()
        .ok_or_else(|| bad("the task never started; it has no folder"))?;
    Ok((main, folder))
}

/// Under the Task lock (ADR 0024): the worktree, `TASK.md` in it, and the
/// record's `branch` and `dir`.
async fn make_worktree(
    env: &Env,
    record: &Record,
    req: &TaskRequest,
    folder: &std::path::Path,
) -> Result<Record, ErrorBody> {
    let config = Arc::clone(&env.records);
    let (id, rev) = (record.id.clone(), record.rev);
    let mut claim =
        blocking(move || config.store()?.claim(&id, rev).map_err(|e| store_error(&e))).await?;
    let work = tokio::time::timeout(tasks::LAUNCH_TIMEOUT, builtin::worktree(req))
        .await
        .map_err(|_| error_body(ErrorCode::TaskFailed, "creating the worktree took too long"))?
        .map_err(|e| task_error(&e))?;
    let (from, to, branch) = (folder.to_path_buf(), work.path.clone(), req.branch.clone());
    blocking(move || {
        brief::copy_into(&from, &to).map_err(|m| error_body(ErrorCode::Internal, m))?;
        claim
            .update(|r| {
                r.branch = Some(branch);
                r.dir = Some(to);
                r.set_read(false);
            })
            .map_err(|e| store_error(&e))?;
        Ok(claim.record().clone())
    })
    .await
}

/// The worktree tags the built-in launcher gives a repo Task's session, so
/// the session groups and closes as one. Only on the Task's own session.
pub(super) async fn tag_worktree(
    client: &muxa_tmux::Client,
    session: muxa_tmux::SessionId,
    main: &std::path::Path,
    dir: &std::path::Path,
    branch: &str,
    task: &str,
) {
    let repo = main
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    for (option, value) in [
        ("@agent_repo", repo.as_str()),
        ("@agent_repo_dir", &main.to_string_lossy()),
        ("@agent_worktree", &dir.to_string_lossy()),
        ("@agent_branch", branch),
    ] {
        if let Err(e) = muxa_tmux::set_session_option(client, session, option, value).await {
            tracing::warn!(task, error = %e, "cannot tag the task's worktree session");
        }
    }
}

fn work_dir(record: &Record) -> Result<PathBuf, ErrorBody> {
    record
        .dir
        .clone()
        .ok_or_else(|| error_body(ErrorCode::Internal, "the branched task lost its directory"))
}

fn bad(message: impl Into<String>) -> ErrorBody {
    error_body(ErrorCode::BadRequest, message)
}
