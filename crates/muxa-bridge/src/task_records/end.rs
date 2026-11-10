//! A Task's end with its worktree (ADR 0043): `update-task` and an Inbox
//! answer with `worktree`. Under the Task's claim it checks what could be
//! lost, closes the session, removes the worktree and branch, then saves the
//! status. A refusal changes nothing.

use std::path::PathBuf;

use muxa_protocol::{CloseMode, ErrorBody, ErrorCode, TaskStatus, WorktreeEnd};
use muxa_tmux::Session;

use super::{Config, Context, blocking, status_of, store_error};
use crate::convert::{self, error_body};
use crate::records::Record;
use crate::records::store::ClaimedTask;
use crate::tasks::{self, TaskLauncher, worktrees};

/// A Task's worktree and the main checkout it belongs to.
struct Target {
    worktree: PathBuf,
    main: PathBuf,
}

/// Ends `task` with `status` (done or dropped). `rev` None takes the
/// record's current one: an Inbox answer.
pub(crate) async fn end(
    cx: &Context,
    task: &str,
    rev: Option<u64>,
    status: TaskStatus,
    choice: WorktreeEnd,
) -> Result<Record, ErrorBody> {
    let new = status_of(status)?;
    let env = cx.env().await?;
    let sessions = muxa_tmux::list_sessions(&env.client)
        .await
        .map_err(|e| convert::error(&e))?;
    let session = sessions
        .into_iter()
        .find(|s| s.task_id.as_deref() == Some(task));
    let mut claim = claim(&cx.records, task, rev).await?;
    let target = target(session.as_ref(), claim.record()).await;
    if let (Some(t), WorktreeEnd::Remove) = (&target, choice) {
        if let Some(why) = worktrees::risk(&t.worktree, &t.main)
            .await
            .map_err(|e| convert::task_error(&e))?
        {
            return Err(error_body(ErrorCode::WouldLoseWork, why));
        }
    }
    let mode = match choice {
        WorktreeEnd::Remove => CloseMode::Close,
        WorktreeEnd::Force => CloseMode::Force,
    };
    match (&session, &target) {
        (Some(s), Some(t)) if s.worktree.is_some() => {
            let launcher = tasks::pick(&cx.tasks, &env.client).await;
            close_session(&env.client, launcher.as_ref(), s, t, mode).await?;
        }
        (Some(s), _) => {
            // A Task without a repo keeps its folder, as ADR 0037 says.
            let dir = claim.record().dir.clone().unwrap_or_default();
            let t = Target {
                worktree: dir.clone(),
                main: dir,
            };
            let builtin = tasks::builtin::Builtin::new(cx.tasks.command.clone());
            close_session(&env.client, &builtin, s, &t, CloseMode::Keep).await?;
        }
        (None, Some(t)) => {
            // A parked Task: no session, only its worktree.
            let force = mode == CloseMode::Force;
            worktrees::Removal::read(&t.main, &t.worktree, force)
                .await
                .run(task)
                .await
                .map_err(|e| convert::task_error(&e))?;
        }
        (None, None) => {}
    }
    let config = cx.records.clone();
    let id = task.to_owned();
    let record = blocking(move || {
        let record = claim
            .update(|r| r.status = new)
            .map_err(|e| store_error(&e))?
            .clone();
        drop(claim);
        crate::items::produce::task_closed(&config, &id);
        Ok(record)
    })
    .await?;
    tracing::info!(
        task,
        rev = record.rev,
        ?choice,
        "task ended with its worktree"
    );
    Ok(record)
}

async fn close_session(
    client: &muxa_tmux::Client,
    launcher: &dyn TaskLauncher,
    s: &Session,
    t: &Target,
    mode: CloseMode,
) -> Result<(), ErrorBody> {
    let session = tasks::TaskSession {
        id: s.id,
        name: s.name.clone(),
        worktree: t.worktree.clone(),
        repo_dir: Some(t.main.clone()),
    };
    launcher
        .close(client, &session, mode)
        .await
        .map_err(|e| convert::task_error(&e))
}

/// The session's worktree, else the record's `dir` when it is a worktree
/// (a parked Task). None for a Task without a repo.
async fn target(session: Option<&Session>, record: &Record) -> Option<Target> {
    let worktree = match session.and_then(|s| s.worktree.as_deref()) {
        Some(w) => PathBuf::from(w),
        None => record.dir.clone().filter(|d| d.join(".git").is_file())?,
    };
    if !worktree.is_dir() {
        return None;
    }
    let main = match session.and_then(|s| s.repo_dir.as_deref()) {
        Some(m) => PathBuf::from(m),
        None => tasks::builtin::main_checkout(&worktree).await.ok()?,
    };
    Some(Target { worktree, main })
}

/// The Task's claim, as close, park and restart take it (ADR 0024).
async fn claim(config: &Config, task: &str, rev: Option<u64>) -> Result<ClaimedTask, ErrorBody> {
    let (config, task) = (config.clone(), task.to_owned());
    blocking(move || {
        let store = config.store()?;
        let rev = match rev {
            Some(rev) => rev,
            None => store.get(&task).map_err(|e| store_error(&e))?.rev,
        };
        let claim = store.claim(&task, rev).map_err(|e| store_error(&e))?;
        if claim
            .record()
            .extra
            .contains_key(crate::records::lifecycle::FIELD)
        {
            return Err(error_body(
                ErrorCode::Conflict,
                "the Task restart requires recovery before it can end",
            ));
        }
        Ok(claim)
    })
    .await
}
