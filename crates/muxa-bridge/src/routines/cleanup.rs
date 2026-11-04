//! After a Routine run published, its session closes. Its folder stays until
//! the Routine's next run publishes, so the last good run can be read. A failed
//! run keeps both (ADR 0037 §2).
use std::path::PathBuf;

use super::runs::load;
use super::store::RoutineStore;
use crate::records::{Status, Store};
use crate::task_records::{Config, Context, close};

/// What one Routine's published runs left.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Leftovers {
    /// The last published run, unless it is the open one: its session closes.
    pub sessions: Vec<String>,
    /// Older published runs' folders, each `<root>/routines/<name>-<id>`.
    pub dirs: Vec<PathBuf>,
}

/// Blocking. Only a done `routine-run` record counts as published: a failed
/// run's record is dropped. A folder is removed only when it is the record's
/// `dir` and the path `routine_dir` checks inside the workspace root.
pub(crate) fn leftovers(config: &Config) -> Leftovers {
    let mut out = Leftovers::default();
    let Some(state_dir) = config.state_dir.as_ref().filter(|d| d.is_dir()) else {
        return out;
    };
    let Ok(workspaces) = load(config) else {
        return out;
    };
    let (Ok(states), Ok(records)) = (RoutineStore::open(state_dir), Store::open(state_dir)) else {
        return out;
    };
    let Ok(all) = records.list() else {
        return out;
    };
    for routine in workspaces.routines() {
        let (Ok(state), Some(ws)) = (
            states.get(&routine.workspace, &routine.name),
            workspaces.get(&routine.workspace),
        ) else {
            continue;
        };
        let Some(last) = state.last_success.as_ref().map(|s| s.task.as_str()) else {
            continue;
        };
        let open = state.running.as_ref().and_then(|r| r.task.as_deref());
        for r in all
            .iter()
            .filter(|r| r.is_routine_run() && r.status == Status::Done)
        {
            if Some(r.id.as_str()) == open {
                continue;
            }
            let Ok(dir) = ws.routine_dir(&routine.name, &r.id) else {
                continue;
            };
            if r.dir.as_deref() != Some(dir.as_path()) {
                continue;
            }
            if r.id == last {
                out.sessions.push(r.id.clone());
            } else if dir.is_dir() {
                out.dirs.push(dir);
            }
        }
    }
    out
}

/// One daemon tick's cleanup. A session closes before its folder could go:
/// the agent runs in the folder.
pub(crate) async fn tick(cx: &Context) {
    let config = std::sync::Arc::clone(&cx.records);
    let left = tokio::task::spawn_blocking(move || leftovers(&config))
        .await
        .unwrap_or_default();
    if !left.sessions.is_empty() {
        close_sessions(cx, &left.sessions).await;
    }
    for dir in left.dirs {
        let shown = dir.display().to_string();
        match tokio::task::spawn_blocking(move || std::fs::remove_dir_all(&dir)).await {
            Ok(Ok(())) => tracing::info!(dir = %shown, "removed an older published run's folder"),
            Ok(Err(e)) => {
                tracing::warn!(dir = %shown, error = %e, "cannot remove a published run's folder")
            }
            Err(e) => tracing::warn!(dir = %shown, error = %e, "removing a run's folder stopped"),
        }
    }
}

async fn close_sessions(cx: &Context, tasks: &[String]) {
    let env = match cx.env().await {
        Ok(env) => env,
        Err(e) => {
            tracing::debug!(error = %e.message, "no tmux for routine cleanup");
            return;
        }
    };
    let sessions = match muxa_tmux::list_sessions(&env.client).await {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!(error = %e, "cannot list sessions for routine cleanup");
            return;
        }
    };
    for task in tasks {
        let Some(s) = sessions
            .iter()
            .find(|s| s.task_id.as_deref() == Some(task.as_str()))
        else {
            continue;
        };
        match close::close_linked(&env.client, &cx.tasks, &cx.records, s, task).await {
            Ok(()) => tracing::info!(task, session = %s.id, "closed a published run's session"),
            Err(e) => {
                tracing::warn!(task, error = %e.message, "cannot close a published run's session")
            }
        }
    }
}
