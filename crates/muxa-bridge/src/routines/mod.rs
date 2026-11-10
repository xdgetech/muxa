//! Routines (ADR 0026): a configured agent run on a trigger that publishes
//! briefings and proposals to Inbox. Config is `records::routines`; a run is
//! a task record of kind `routine-run` that reuses the Task machinery; this
//! module schedules runs, checks and publishes their output, and serves the
//! `work` level 1 requests.

pub mod brief;
pub mod check;
pub mod cleanup;
pub mod inbox;
pub mod output;
pub mod record;
pub mod runs;
pub mod schedule;
pub mod store;

use std::path::Path;
use std::sync::Arc;

use muxa_protocol::{
    ErrorBody, ErrorCode, Message, Request, RoutineInfo, RoutineProblem, RoutineTrigger,
};

use crate::convert::error_body;
use crate::records::Record;
use crate::records::routines::Routine;
use crate::task_records::{Config, Context};
use runs::{StartError, When};
use store::{RoutineState, RoutineStore};

pub use runs::tick;

/// Answers a `work` level 1 request: its list or saved message, and its error.
pub async fn handle(cx: &Context, req: Request) -> (Vec<Message>, Option<ErrorBody>) {
    if let Request::UpdateInbox {
        id,
        entry,
        action: muxa_protocol::InboxAction::AskAgent,
        text,
        ..
    } = &req
    {
        return match crate::items::ask_agent::ask(cx, *id, entry, text.as_deref()).await {
            Ok(messages) => (messages, None),
            Err(e) => (Vec::new(), Some(e)),
        };
    }
    if let Request::UpdateInbox {
        id,
        entry,
        action: muxa_protocol::InboxAction::Approve,
        ..
    } = &req
    {
        if entry.starts_with("i-") {
            if let Some(answer) = crate::assistant::proposals::answer(cx, *id, entry).await {
                return answer;
            }
        }
    }
    let result = match req {
        Request::ListRoutines { id } => {
            let config = Arc::clone(&cx.records);
            blocking(move || list(&config))
                .await
                .map(|(entries, problems)| Message::RoutineList {
                    routines: id,
                    entries,
                    problems,
                })
        }
        Request::RunRoutine {
            id,
            workspace,
            routine,
        } => run_now(cx, id, &workspace, &routine).await,
        Request::ListInbox { id } => {
            let config = Arc::clone(&cx.records);
            blocking(move || {
                let entries = inbox::list(&config)?;
                let items = crate::items::list(&config, &entries)?;
                let assistant = crate::assistant::list(&config)?;
                let plans =
                    crate::assistant::plan::list(&config, &assistant.work, &assistant.requests)?;
                // A broken workspaces file is logged by the load; the list still answers.
                let info = config
                    .load_workspaces()
                    .ok()
                    .and_then(|w| crate::assistant::info::info(&w));
                Ok((entries, items, assistant, plans, info))
            })
            .await
            .map(
                |(entries, items, assistant, plans, info)| Message::InboxList {
                    inbox: id,
                    entries,
                    items,
                    work: assistant.work,
                    captures: assistant.captures,
                    requests: assistant.requests,
                    plans,
                    assistant: info.map(Box::new),
                    // Policies come with level 10 (ADR 0048).
                    policies: Vec::new(),
                    hypotheses: Vec::new(),
                    corrections: Vec::new(),
                },
            )
        }
        Request::UpdateInbox {
            id,
            entry,
            action,
            text,
            option,
            worktree,
            ..
        } if entry.starts_with("i-") => {
            // `until` is ignored: Later moves an item down (ADR 0040).
            let answer = crate::items::answer::Answer {
                item: entry,
                action,
                text,
                option,
                worktree,
            };
            crate::items::answer::update(cx, answer)
                .await
                .map(|item| Message::InboxSaved {
                    inbox: id,
                    entry: None,
                    item: Some(Box::new(item)),
                })
        }
        Request::UpdateInbox {
            id, entry, action, ..
        } => inbox::update(cx, entry, action)
            .await
            .map(|entry| Message::InboxSaved {
                inbox: id,
                item: Some(Box::new(crate::items::from_entry(&entry))),
                entry: Some(Box::new(entry)),
            }),
        _ => Err(error_body(ErrorCode::Unsupported, "not a routine request")),
    };
    match result {
        Ok(m) => (vec![m], None),
        Err(e) => (Vec::new(), Some(e)),
    }
}

async fn run_now(
    cx: &Context,
    id: u64,
    workspace: &str,
    routine: &str,
) -> Result<Message, ErrorBody> {
    let state = runs::start(cx, workspace, routine, When::Now(crate::records::now()))
        .await
        .map_err(|e| start_error(&e))?;
    let config = Arc::clone(&cx.records);
    let (ws, name) = (workspace.to_owned(), routine.to_owned());
    blocking(move || {
        let workspaces = runs::load(&config).map_err(|e| start_error(&e))?;
        let routine = workspaces
            .routine(&ws, &name)
            .ok_or_else(|| error_body(ErrorCode::BadRequest, "the Routine was removed"))?;
        Ok(Message::RoutineSaved {
            routine: id,
            entry: info(routine, &state, crate::records::now()),
        })
    })
    .await
}

fn start_error(e: &StartError) -> ErrorBody {
    let code = match e {
        StartError::Bad(_) => ErrorCode::BadRequest,
        StartError::Busy(_) | StartError::Handled => ErrorCode::Conflict,
        StartError::Unsupported(_) => ErrorCode::Unsupported,
        StartError::Failed(_) => ErrorCode::TaskFailed,
    };
    error_body(code, e.to_string())
}

/// Blocking. Every configured Routine with its state, in file order, and
/// the workspaces whose routines file is broken.
pub fn list(config: &Config) -> Result<(Vec<RoutineInfo>, Vec<RoutineProblem>), ErrorBody> {
    let workspaces = runs::load(config).map_err(|e| start_error(&e))?;
    let store = match config.state_dir.as_ref().filter(|d| d.is_dir()) {
        Some(dir) => Some(RoutineStore::open(dir).map_err(|e| {
            tracing::warn!(error = %e, "cannot open routine state");
            error_body(ErrorCode::Internal, e.to_string())
        })?),
        None => None,
    };
    let at = crate::records::now();
    let entries = workspaces
        .routines()
        .iter()
        .map(|r| {
            let state = store
                .as_ref()
                .and_then(|s| s.get(&r.workspace, &r.name).ok())
                .unwrap_or_else(|| RoutineState::new(&r.workspace, &r.name));
            info(r, &state, at)
        })
        .collect();
    let problems = workspaces
        .routine_problems()
        .iter()
        .map(|p| {
            tracing::warn!(workspace = %p.workspace, reason = %p.reason, "routines file unusable");
            RoutineProblem {
                workspace: p.workspace.clone(),
                reason: p.reason.clone(),
            }
        })
        .collect();
    Ok((entries, problems))
}

/// The wire form. `nextDue` only for an enabled Routine with a trigger.
pub fn info(r: &Routine, state: &RoutineState, at: u64) -> RoutineInfo {
    let next_due = match (&r.trigger, r.enabled) {
        (Some(t), true) => jiff::Timestamp::from_second(at as i64)
            .ok()
            .and_then(|now| schedule::next_due(t, now, runs::last_date(state)).ok())
            .and_then(|ts| u64::try_from(ts.as_second()).ok()),
        _ => None,
    };
    RoutineInfo {
        workspace: r.workspace.clone(),
        name: r.name.clone(),
        trigger: r.trigger.as_ref().map(|t| RoutineTrigger {
            at: t.at(),
            timezone: t.timezone.clone(),
            schedule: Some(t.schedule.clone()),
        }),
        enabled: r.enabled,
        worker: r.worker.clone(),
        project: r.project.clone(),
        next_due,
        last_success: state.last_success.clone(),
        last_failure: state.last_failure.clone(),
        running: state.running.clone(),
    }
}

/// Blocking. Before the agent starts: the private stage folder (0700) and
/// the state the last successful run committed. Called by `create-task`'s path.
pub fn prepare(config: &Config, record: &Record, dir: &Path) -> Result<(), String> {
    use std::os::unix::fs::DirBuilderExt;
    let run = record
        .routine_run()
        .ok_or("the record is not a Routine run")?;
    let stage = dir.join(output::STAGE_DIR);
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(&stage)
        .map_err(|e| format!("cannot create {}: {e}", stage.display()))?;
    let state_dir = runs::dir(config).map_err(|e| e.to_string())?;
    let state = RoutineStore::open(&state_dir)
        .and_then(|s| s.get(&record.workspace, &run.name))
        .map_err(|e| e.to_string())?;
    if let Some(value) = state.state {
        let path = dir.join(output::STATE);
        let bytes = serde_json::to_vec_pretty(&value).map_err(|e| e.to_string())?;
        std::fs::write(&path, bytes)
            .map_err(|e| format!("cannot write {}: {e}", path.display()))?;
    }
    Ok(())
}

async fn blocking<T: Send + 'static>(
    f: impl FnOnce() -> Result<T, ErrorBody> + Send + 'static,
) -> Result<T, ErrorBody> {
    tokio::task::spawn_blocking(f)
        .await
        .unwrap_or_else(|_| Err(error_body(ErrorCode::Internal, "the routine store stopped")))
}
