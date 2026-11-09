//! The daemon's tick for assistant requests (ADR 0046 §4–§6): finds a run
//! again after a crash, keeps each version's delivery, ends what a version
//! no longer needs, and closes finished runs' sessions as for Routine runs
//! (ADR 0037). It never stops a timed-out agent.
use std::sync::Arc;

use muxa_protocol::{AssistantRequest, Delivery};
use muxa_tmux::{AgentState, Role, Session};

use super::delivery::{self, Agent, Facts};
use super::store::Shelf;
use crate::records::{Record, Status, Store, StoreError};
use crate::task_records::{Config, Context};

/// Seconds after which a claimed version without a run did not start.
const START_LIMIT: u64 = 300;
const WRITE_ATTEMPTS: usize = 3;

/// What one scan found to do outside the store.
#[derive(Debug, Default)]
pub(super) struct Plan {
    /// `(request, run)` whose delivery needs the adapter.
    watch: Vec<(String, String)>,
    /// Runs whose session closes: published, replaced or cancelled.
    close: Vec<String>,
    /// Published runs' folders to remove.
    dirs: Vec<std::path::PathBuf>,
}

pub async fn tick(cx: &Context) {
    let config = Arc::clone(&cx.records);
    let now = crate::records::now();
    let plan = tokio::task::spawn_blocking(move || scan(&config, now))
        .await
        .unwrap_or_else(|e| {
            tracing::warn!(error = %e, "the assistant scan stopped");
            Plan::default()
        });
    if !plan.watch.is_empty() || !plan.close.is_empty() {
        if let Some(sessions) = sessions(cx).await {
            watch(cx, &plan.watch, &sessions).await;
            close(cx, &plan.close, &sessions).await;
        }
    }
    super::cleanup::remove(plan.dirs).await;
}

/// Blocking. Every request once: writes what the store alone decides.
fn scan(config: &Config, now: u64) -> Plan {
    let mut plan = Plan::default();
    let Some(state_dir) = config.state_dir.as_ref().filter(|d| d.is_dir()) else {
        return plan;
    };
    let opened = Shelf::<AssistantRequest>::open(state_dir).and_then(|s| Ok((s.all()?, s)));
    let (all, shelf) = match opened {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!(error = %e, "cannot read assistant requests");
            return plan;
        }
    };
    if all.is_empty() {
        return plan;
    }
    let records: Vec<Record> = config
        .records_if_any()
        .into_iter()
        .filter(Record::is_assistant_run)
        .collect();
    let workspaces = config.load_workspaces().unwrap_or_default();
    for q in all {
        let timeout = delivery::timeout(&workspaces, &q.workspace);
        one(config, &shelf, &q, &records, timeout, now, &mut plan);
        let runs: Vec<&Record> = records
            .iter()
            .filter(|r| r.assistant_run().is_some_and(|run| run.request == q.id))
            .collect();
        // A run of the current version may still be starting: only older ones end.
        let older = |r: &&&Record| r.assistant_run().is_some_and(|run| run.version < q.version);
        for r in runs.iter().filter(older) {
            if super::ask::open(r) {
                drop_run(
                    config,
                    &r.id,
                    &format!("Version {} replaced this run.", q.version),
                );
            }
            if closable(r) {
                plan.close.push(r.id.clone());
            }
        }
        plan.dirs
            .extend(super::cleanup::leftovers(&workspaces, &q, &runs, now));
    }
    plan
}

/// One request's own run: found again, failed, closed, or its delivery kept.
fn one(
    config: &Config,
    shelf: &Shelf<AssistantRequest>,
    q: &AssistantRequest,
    records: &[Record],
    timeout: u64,
    now: u64,
    plan: &mut Plan,
) {
    let Some(task) = q.run.clone() else {
        unstarted(shelf, q, records, now);
        return;
    };
    let Some(record) = records.iter().find(|r| r.id == task) else {
        return;
    };
    if q.delivery == Delivery::CancelRequested {
        if super::ask::open(record) {
            drop_run(config, &task, "The request was cancelled.");
        }
        if closable(record) {
            plan.close.push(task);
        } else if record.session.is_none() {
            if let Err(e) = super::cancel::cancelled(config, &q.id, q.version) {
                tracing::warn!(request = %q.id, error = %e.message, "cannot end a cancel");
            }
        }
        return;
    }
    let facts = Facts {
        record: Some(record),
        agent: None,
        timeout,
        now,
    };
    let (state, at) = delivery::derive(q, &facts);
    match state {
        Delivery::Received | Delivery::Processing => plan.watch.push((q.id.clone(), task.clone())),
        Delivery::Failed if super::ask::open(record) => {
            drop_run(config, &task, "The request failed: no valid result.");
        }
        Delivery::Resulted if closable(record) && record.status == Status::Done => {
            plan.close.push(task.clone())
        }
        _ => {}
    }
    save(shelf, q, state, at);
}

/// A claimed version without a run: found again by its marker, or failed
/// once nobody can still make it.
fn unstarted(shelf: &Shelf<AssistantRequest>, q: &AssistantRequest, records: &[Record], now: u64) {
    if delivery::settled(q.delivery) && q.delivery != Delivery::CancelRequested {
        return;
    }
    let found = super::ask::existing(records, &q.id, q.version).map(|r| r.id.clone());
    let changed = shelf.update_with_extra(
        &q.id,
        |q, extra| -> Result<_, crate::routines::store::StateError> {
            if q.run.is_some() {
                return Ok(None);
            }
            let claim = super::ask::creating(extra, q.version);
            if let Some(task) = &found {
                q.run = Some(task.clone());
                extra.remove(super::ask::CREATING_FIELD);
                q.updated_at = now;
                return Ok(Some("assistant run found again"));
            }
            let fresh =
                claim.is_some_and(|since| now.saturating_sub(since) < super::ask::CREATING_LIMIT);
            if q.delivery == Delivery::CancelRequested && !fresh {
                q.delivery = Delivery::Cancelled;
            } else if now.saturating_sub(claim.unwrap_or(q.updated_at)) > START_LIMIT {
                q.delivery = Delivery::Failed;
            } else {
                return Ok(None);
            }
            extra.remove(super::ask::CREATING_FIELD);
            q.delivery_at = now;
            q.updated_at = now;
            Ok(Some("assistant run did not start"))
        },
    );
    match changed {
        Ok(Some(Some(what))) => tracing::info!(request = %q.id, version = q.version, "{what}"),
        Ok(_) => {}
        Err(e) => tracing::warn!(request = %q.id, error = %e, "cannot settle an unstarted request"),
    }
}

/// A session to close. A send or restart awaiting recovery holds its record:
/// closing waits for `recover-restart` (ADR 0024) instead of failing each tick.
fn closable(r: &Record) -> bool {
    r.session.is_some() && !r.extra.contains_key(crate::records::lifecycle::FIELD)
}

/// Writes a changed delivery, unless the request moved on meanwhile.
fn save(shelf: &Shelf<AssistantRequest>, seen: &AssistantRequest, state: Delivery, at: u64) {
    if (state, at) == (seen.delivery, seen.delivery_at) {
        return;
    }
    let saved = shelf.update(
        &seen.id,
        |q| -> Result<_, crate::routines::store::StateError> {
            let same = q.version == seen.version && q.delivery == seen.delivery;
            if same {
                q.delivery = state;
                q.delivery_at = at;
            }
            Ok(same)
        },
    );
    match saved {
        Ok(Some(true)) if state != seen.delivery => {
            tracing::info!(request = %seen.id, version = seen.version, delivery = ?state, "assistant delivery")
        }
        Ok(_) => {}
        Err(e) => tracing::warn!(request = %seen.id, error = %e, "cannot save a delivery"),
    }
}

/// Every session, linked to records, with adapter states.
async fn sessions(cx: &Context) -> Option<Vec<Session>> {
    let env = match cx.env().await {
        Ok(env) => env,
        Err(e) => {
            tracing::debug!(error = %e.message, "no tmux for assistant runs");
            return None;
        }
    };
    let mut sessions = match muxa_tmux::list_sessions(&env.client).await {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!(error = %e, "cannot list sessions for assistant runs");
            return None;
        }
    };
    let (config, agents) = (Arc::clone(&cx.records), Arc::clone(&cx.agents));
    tokio::task::spawn_blocking(move || {
        crate::task_records::link(&mut sessions, &config);
        crate::sessions::fill_agents(&mut sessions, &agents);
        sessions
    })
    .await
    .ok()
}

/// The adapter's word on `task`'s agent; None without one (invariant 6).
fn agent_of(sessions: &[Session], task: &str) -> Option<Agent> {
    let s = sessions
        .iter()
        .find(|s| s.task_id.as_deref() == Some(task))?;
    let pane = s.panes.iter().find(|p| p.role == Role::Agent && !p.dead)?;
    if pane.shell_foreground {
        return Some(Agent::NotBusy);
    }
    let status = pane.agent.as_ref()?;
    Some(match status.state {
        AgentState::Running => Agent::Busy(status.since.unwrap_or_else(crate::records::now)),
        _ => Agent::NotBusy,
    })
}

async fn watch(cx: &Context, watched: &[(String, String)], sessions: &[Session]) {
    let seen: Vec<(String, String, Option<Agent>)> = watched
        .iter()
        .map(|(q, t)| (q.clone(), t.clone(), agent_of(sessions, t)))
        .collect();
    let config = Arc::clone(&cx.records);
    let done = tokio::task::spawn_blocking(move || {
        let Some(dir) = config.state_dir.as_ref() else {
            return;
        };
        let (Ok(shelf), Ok(store)) = (Shelf::<AssistantRequest>::open(dir), Store::open(dir))
        else {
            return;
        };
        let workspaces = config.load_workspaces().unwrap_or_default();
        let now = crate::records::now();
        for (id, task, agent) in seen {
            let (Ok(Some(q)), Ok(record)) = (shelf.get(&id), store.get(&task)) else {
                continue;
            };
            let facts = Facts {
                record: Some(&record),
                agent,
                timeout: delivery::timeout(&workspaces, &q.workspace),
                now,
            };
            let (state, at) = delivery::derive(&q, &facts);
            save(&shelf, &q, state, at);
        }
    });
    if let Err(e) = done.await {
        tracing::warn!(error = %e, "the assistant delivery update stopped");
    }
}

/// Closes each run's session, then forgets it on the record, so the next
/// tick neither closes it again nor keeps its folder.
async fn close(cx: &Context, tasks: &[String], sessions: &[Session]) {
    for task in tasks {
        if let Some(s) = sessions
            .iter()
            .find(|s| s.task_id.as_deref() == Some(task.as_str()))
        {
            let Ok(env) = cx.env().await else { return };
            let closed = crate::task_records::close::close_linked(
                &env.client,
                &cx.tasks,
                &cx.records,
                s,
                task,
            )
            .await;
            match closed {
                Ok(()) => {
                    tracing::info!(task, session = %s.id, "closed an assistant run's session")
                }
                Err(e) => {
                    tracing::warn!(task, error = %e.message, "cannot close an assistant run's session");
                    continue;
                }
            }
        }
        let (config, t) = (Arc::clone(&cx.records), task.clone());
        let forgot = tokio::task::spawn_blocking(move || {
            update_run(&config, &t, |r| r.session = None);
        });
        if let Err(e) = forgot.await {
            tracing::warn!(task, error = %e, "forgetting a closed session stopped");
        }
    }
}

/// Whether a session runs `task`; None when tmux cannot tell.
pub async fn has_session(cx: &Context, task: &str) -> Option<bool> {
    let env = cx.env().await.ok()?;
    let sessions = muxa_tmux::list_sessions(&env.client).await.ok()?;
    let found = sessions.iter().any(|s| s.task_id.as_deref() == Some(task));
    if !found {
        let (config, t) = (Arc::clone(&cx.records), task.to_owned());
        let _ = tokio::task::spawn_blocking(move || update_run(&config, &t, |r| r.session = None))
            .await;
    }
    Some(found)
}

/// Blocking. An open run's record ends `dropped`, with why.
pub fn drop_run(config: &Config, task: &str, note: &str) {
    update_run(config, task, |r| {
        if super::ask::open(r) {
            r.status = Status::Dropped;
            r.note = Some(note.to_owned());
        }
    });
}

/// Blocking. Changes an assistant run's record, retrying a concurrent write.
fn update_run(config: &Config, task: &str, change: impl Fn(&mut Record)) {
    let Some(Ok(store)) = config.state_dir.as_ref().map(Store::open) else {
        return;
    };
    for _ in 0..WRITE_ATTEMPTS {
        let Ok(r) = store.get(task) else { return };
        if !r.is_assistant_run() {
            return;
        }
        let mut probe = r.clone();
        change(&mut probe);
        if probe == r {
            return;
        }
        match store.update(task, r.rev, &change) {
            Ok(_) => return,
            Err(StoreError::Conflict { .. }) => continue,
            Err(e) => {
                tracing::warn!(task, error = %e, "cannot update an assistant run's record");
                return;
            }
        }
    }
}
