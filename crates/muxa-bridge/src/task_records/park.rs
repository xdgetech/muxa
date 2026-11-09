//! Parking (ADR 0034 §4): `park-task` writes `HANDOFF.md` and closes the
//! Task's session, and keeps its record, `TASK.md` and directory. Resume is
//! `restart-agent` on a Task with no session: a new session in its `dir`.
//! The daemon parks a Task idle past its workspace's `park_after`.
use std::path::Path;
use std::sync::Arc;

use muxa_protocol::{ErrorBody, ErrorCode};
use muxa_tmux::{AgentState, Client, Role, Session, SessionId};

use super::create::Env;
use super::handoff::{self, Linked};
use super::{Config, blocking, brief, facts, link, store_error};
use crate::convert::error_body;
use crate::items::store::ItemStore;
use crate::records::{PendingPrompt, PromptKind, Record, RunReason, Status, now};
use crate::tasks::{builtin, session_name};

/// The session option that links a session to its record.
const TAG: &str = "@muxa_task";

pub async fn park(env: &Env, task: String) -> Result<Record, ErrorBody> {
    let record = get(&env.records, task).await?;
    let dir = parkable(&record)?;
    let session = linked_session(&env.client, &env.records, &record.id)
        .await?
        .ok_or_else(|| bad(format!("task {} has no session; it is parked", record.id)))?;
    let config = Arc::clone(&env.records);
    let (id, rev) = (record.id.clone(), record.rev);
    let claim =
        blocking(move || config.store()?.claim(&id, rev).map_err(|e| store_error(&e))).await?;
    let (snapshot, at) = (claim.record().clone(), dir.clone());
    blocking(move || write_handoff(&snapshot, &at)).await?;
    close_session(&env.client, session).await?;
    let saved = blocking(move || {
        let mut claim = claim;
        claim
            .update(|r| {
                let end = now();
                if let Some(run) = r.runs.last_mut().filter(|run| run.ended_at.is_none()) {
                    run.ended_at = Some(end.max(run.started_at));
                }
                r.session = None;
                r.set_pending_prompt(None);
            })
            .map_err(|e| store_error(&e))?;
        Ok(claim.record().clone())
    })
    .await?;
    tracing::info!(task = %saved.id, session = %session, "task parked");
    Ok(saved)
}

async fn get(config: &Arc<Config>, task: String) -> Result<Record, ErrorBody> {
    let config = Arc::clone(config);
    blocking(move || config.store()?.get(&task).map_err(|e| store_error(&e))).await
}

/// The directory of an open Task that parking and resuming support.
fn parkable(record: &Record) -> Result<std::path::PathBuf, ErrorBody> {
    if !matches!(record.status, Status::Open | Status::Review) {
        return Err(bad(format!("task {} is closed", record.id)));
    }
    if record.worker().is_some() {
        return Err(bad(
            "a task with a named worker cannot be parked or resumed yet",
        ));
    }
    if record.is_background_run() {
        return Err(bad("a Routine or assistant run is not parked"));
    }
    record.dir.clone().ok_or_else(|| {
        bad(format!(
            "task {} never started; it has no directory",
            record.id
        ))
    })
}

/// Blocking.
fn write_handoff(record: &Record, dir: &Path) -> Result<(), ErrorBody> {
    let since = record
        .open_run()
        .map_or(record.created_at, |r| r.started_at);
    let mut text = facts::render_parked(record, &facts::work(record, dir, since), now());
    if let Some(folder) = record.earlier_folder() {
        text.push_str(&facts::folder_text(&folder, since));
    }
    brief::write_file(dir, facts::FILE, &text)
        .map(drop)
        .map_err(|m| error_body(ErrorCode::Internal, m))
}

/// The clients on it move to another session first, as closing a task
/// does. Its device sessions are killed before it (invariant 2).
async fn close_session(client: &Client, session: SessionId) -> Result<(), ErrorBody> {
    let tmux = |e: muxa_tmux::Error| crate::convert::error(&e);
    let other = muxa_tmux::last_other_session(client, session)
        .await
        .map_err(tmux)?
        .ok_or_else(|| {
            error_body(
                ErrorCode::TaskFailed,
                "the task's session is the only one; parking it would stop tmux",
            )
        })?;
    for c in muxa_tmux::clients_of(client, session).await.map_err(tmux)? {
        muxa_tmux::switch_client(client, &c, other)
            .await
            .map_err(tmux)?;
    }
    muxa_tmux::kill_session(client, session).await.map_err(tmux)
}

/// The session that runs `task`, by its tag or its `dir` (ADR 0015).
pub(super) async fn linked_session(
    client: &Client,
    config: &Arc<Config>,
    task: &str,
) -> Result<Option<SessionId>, ErrorBody> {
    let sessions = muxa_tmux::list_sessions(client)
        .await
        .map_err(|e| crate::convert::error(&e))?;
    let (config, task) = (Arc::clone(config), task.to_owned());
    blocking(move || {
        let mut sessions = sessions;
        link(&mut sessions, &config);
        Ok(sessions
            .into_iter()
            .find(|s| s.task_id.as_deref() == Some(task.as_str()))
            .map(|s| s.id))
    })
    .await
}

/// Resume: a new session in the Task's `dir` running the workspace's agent,
/// tagged, with a restart run and its prompt (ADR 0018). Under the Task lock.
pub(super) async fn resume(env: &Env, record: Record) -> Result<Record, ErrorBody> {
    let dir = parkable(&record)?;
    if !dir.is_dir() {
        return Err(error_body(
            ErrorCode::TaskFailed,
            format!("{} is gone; the task cannot resume there", dir.display()),
        ));
    }
    let config = Arc::clone(&env.records);
    let workspace = record.workspace.clone();
    let agent = blocking(move || {
        Ok(config
            .load_workspaces()?
            .get(&workspace)
            .and_then(|w| w.agent.clone()))
    })
    .await?;
    let config = Arc::clone(&env.records);
    let (id, rev) = (record.id.clone(), record.rev);
    let claim =
        blocking(move || config.store()?.claim(&id, rev).map_err(|e| store_error(&e))).await?;
    let name = match &record.branch {
        Some(branch) => session_name(branch),
        None => dir
            .file_name()
            .map(|n| session_name(&n.to_string_lossy()))
            .unwrap_or_else(|| record.id.clone()),
    };
    let tmux = |e: muxa_tmux::Error| crate::convert::error(&e);
    if muxa_tmux::session_names(&env.client)
        .await
        .map_err(tmux)?
        .contains(&name)
    {
        return Err(error_body(
            ErrorCode::TaskFailed,
            format!("a session named {name} exists but does not run this task; rename it"),
        ));
    }
    let session = builtin::Builtin::new(env.tasks.command.clone())
        .start(&env.client, &name, &dir, agent.as_ref())
        .await
        .map_err(|e| crate::convert::task_error(&e))?;
    muxa_tmux::set_session_option(&env.client, session, TAG, &record.id)
        .await
        .map_err(tmux)?;
    if let (Some(main), Some(branch)) = (&record.repo, &record.branch) {
        super::branch_task::tag_worktree(&env.client, session, main, &dir, branch, &record.id)
            .await;
    }
    let (at, session_name) = (dir.clone(), name.clone());
    let saved = blocking(move || {
        let mut claim = claim;
        brief::refresh(&at, claim.record()).map_err(|m| error_body(ErrorCode::Internal, m))?;
        claim
            .update(|r| {
                let since = now();
                r.session = Some(session_name);
                let run = r.start_run(RunReason::Restart, since);
                r.set_pending_prompt(Some(PendingPrompt {
                    kind: PromptKind::Restart,
                    since,
                    run,
                }));
            })
            .map_err(|e| store_error(&e))?;
        Ok(claim.record().clone())
    })
    .await?;
    tracing::info!(task = %saved.id, session = %session, "parked task resumed");
    if let Some(pending) = saved.pending_prompt() {
        env.pending.spawn(handoff::send_prompt(
            Arc::clone(&env.client),
            Arc::clone(&env.agents),
            Arc::clone(&env.records),
            handoff::Wait {
                task: saved.id.clone(),
                which: Linked::Session(session),
                pending,
            },
        ));
    }
    Ok(saved)
}

/// Blocking. The open Tasks to park now: their workspace sets `park_after`,
/// their agent has been `idle` or `done` that long in this run, and they
/// have no open or later Inbox item, loop, prompt wait or restart.
pub(crate) fn due(sessions: &[Session], config: &Config, at: u64) -> Vec<String> {
    let Ok(workspaces) = config.load_workspaces() else {
        return Vec::new();
    };
    let Some(state) = config.state_dir.as_ref().filter(|d| d.is_dir()) else {
        return Vec::new();
    };
    let items = match ItemStore::open(state) {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!(error = %e, "cannot open the inbox items to park tasks");
            return Vec::new();
        }
    };
    config
        .records_if_any()
        .into_iter()
        .filter(|r| {
            let Some(after) = workspaces.get(&r.workspace).and_then(|w| w.park_after) else {
                return false;
            };
            let Some(session) = sessions
                .iter()
                .find(|s| s.task_id.as_deref() == Some(&r.id))
            else {
                return false;
            };
            quiet(r, &items) && idle_since(session, r).is_some_and(|t| at >= t + after)
        })
        .map(|r| r.id)
        .collect()
}

fn quiet(r: &Record, items: &ItemStore) -> bool {
    use muxa_protocol::ItemState;
    let open_item = items.get(&r.id).is_ok_and(|t| {
        t.items
            .iter()
            .any(|s| matches!(s.item.state, ItemState::Open | ItemState::Later))
    });
    matches!(r.status, Status::Open | Status::Review)
        && r.worker().is_none()
        && !r.is_background_run()
        && r.pending_prompt().is_none()
        && !r.extra.contains_key(crate::records::lifecycle::FIELD)
        && !r.task_loop().ok().flatten().is_some_and(|l| l.active())
        && !open_item
}

/// When the agent's current `idle` or `done` began, in its current run.
fn idle_since(session: &Session, r: &Record) -> Option<u64> {
    let pane = session
        .panes
        .iter()
        .find(|p| p.role == Role::Agent && !p.dead)?;
    let status = crate::loops::step::current(pane.agent, r.runs.last().map(|run| run.started_at))?;
    matches!(status.state, AgentState::Idle | AgentState::Done)
        .then_some(status.since)
        .flatten()
}

fn bad(message: impl Into<String>) -> ErrorBody {
    error_body(ErrorCode::BadRequest, message)
}

#[cfg(test)]
mod tests {
    use super::*;
    use muxa_protocol::Message;

    #[test]
    fn only_an_open_task_with_a_directory_and_no_worker_parks() {
        let mut r: Record = serde_json::from_value(serde_json::json!({
            "version":1,"id":"t-000001","rev":1,"workspace":"w","title":"t",
            "goal":"","status":"open","createdAt":1,"updatedAt":1
        }))
        .unwrap();
        assert!(parkable(&r).is_err(), "never started");
        r.dir = Some("/w/tasks/t-000001".into());
        assert!(parkable(&r).is_ok());
        r.extra
            .insert("worker".into(), serde_json::json!("Reviewer"));
        assert!(parkable(&r).is_err());
        r.extra.remove("worker");
        r.status = Status::Done;
        assert!(parkable(&r).is_err());
    }

    /// ADR 0046 section 5: only the Task is listed, watched or parked.
    #[tokio::test]
    async fn only_a_task_of_each_record_kind_shows_as_a_task() {
        let base = std::env::temp_dir().join(format!(
            "muxa-kinds-{}-{:08x}",
            std::process::id(),
            getrandom::u32().unwrap()
        ));
        std::fs::create_dir_all(&base).unwrap();
        let config = Config {
            state_dir: Some(base.clone()),
            workspaces: None,
            home: None,
        };
        let store = config.store().unwrap();
        let items = ItemStore::open(&base).unwrap();
        let mut task = None;
        let kind = |k: &str| {
            let mut extra = serde_json::Map::new();
            extra.insert(crate::records::KIND_FIELD.into(), k.into());
            Some(extra)
        };
        // ADR 0047 §5: an overall run is an assistant run that names a plan.
        let plan = crate::assistant::record::RunFor::Plan(crate::assistant::record::PlanRun {
            plan: "p-0000000a".into(),
            notes_base: "e3b0".into(),
        });
        for extra in [
            None,
            kind("routine-run"),
            kind("assistant-run"),
            Some(plan.fields()),
            kind("future-run"),
        ] {
            let shown = extra.is_none();
            let extra = extra.unwrap_or_default();
            let kind = extra.get(crate::records::KIND_FIELD).cloned();
            let made = store
                .create(crate::records::NewRecord {
                    workspace: "w".into(),
                    title: "t".into(),
                    dir: Some(base.join("dir")),
                    extra,
                    ..Default::default()
                })
                .unwrap();
            assert_eq!(parkable(&made).is_ok(), shown, "{kind:?}");
            assert_eq!(quiet(&made, &items), shown, "{kind:?}");
            assert_eq!(crate::items::produce::watched(&made), shown, "{kind:?}");
            if shown {
                task = Some(made.id);
            }
        }
        let Message::TaskList { entries, .. } = super::super::list(&config, 1).await.unwrap()
        else {
            panic!("not a task-list")
        };
        let listed: Vec<String> = entries.into_iter().map(|t| t.id).collect();
        assert_eq!(listed, vec![task.unwrap()]);
        std::fs::remove_dir_all(base).unwrap();
    }
}
