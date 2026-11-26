//! Paused Tasks (ADR 0104): pause keeps a record of the agent, then ends it
//! and its shells; resume runs it again from the record. The host keeps the
//! records; [`MemoryPaused`] holds them in memory for every host.

use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use muxa_backend::{BackendError, SessionBackend, Size, TerminalId, TerminalInfo};
use muxa_inbox::Inbox;
use muxa_protocol::v2::{ErrorCode, LoopSpec, Paused, WorktreeMode};
use tokio::sync::watch;

use crate::session::{code_of, task_code, to_wire};
use crate::shells;
use crate::task_actions::{Refusal, done_all, end_agent, places_of, stop_loop};
use crate::task_items::is_agent;
use crate::tasks::{self, DIR, Host};

/// Loop states a pause stops and a resume arms again.
const ACTIVE: [&str; 3] = ["armed", "checking", "paused"];

/// The paused Tasks in memory, oldest first, with a revision for `watch`.
/// A host that keeps them on disk saves inside [`MemoryPaused::keep`] and
/// [`MemoryPaused::remove`], so memory never runs ahead of the file.
pub struct MemoryPaused {
    list: Mutex<Vec<Paused>>,
    changes: watch::Sender<u64>,
}

impl MemoryPaused {
    pub fn new(list: Vec<Paused>) -> Self {
        Self {
            list: Mutex::new(list),
            changes: watch::Sender::new(0),
        }
    }

    pub fn list(&self) -> Vec<Paused> {
        self.lock().clone()
    }

    /// Adds `paused`, or replaces the record with its id, after `save`
    /// accepted the new list.
    pub fn keep<E>(
        &self,
        paused: Paused,
        save: impl FnOnce(&[Paused]) -> Result<(), E>,
    ) -> Result<(), E> {
        self.update(save, |list| {
            list.retain(|p| p.terminal.id != paused.terminal.id);
            list.push(paused);
            true
        })
    }

    /// Removes the record `id`, after `save` accepted the new list. An id
    /// that is not paused changes nothing.
    pub fn remove<E>(
        &self,
        id: &str,
        save: impl FnOnce(&[Paused]) -> Result<(), E>,
    ) -> Result<(), E> {
        self.update(save, |list| {
            let before = list.len();
            list.retain(|p| p.terminal.id != id);
            list.len() != before
        })
    }

    pub fn changes(&self) -> watch::Receiver<u64> {
        self.changes.subscribe()
    }

    fn update<E>(
        &self,
        save: impl FnOnce(&[Paused]) -> Result<(), E>,
        change: impl FnOnce(&mut Vec<Paused>) -> bool,
    ) -> Result<(), E> {
        let mut list = self.lock();
        let mut next = list.clone();
        if !change(&mut next) {
            return Ok(());
        }
        save(&next)?;
        *list = next;
        drop(list);
        self.changes.send_modify(|r| *r += 1);
        Ok(())
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Vec<Paused>> {
        // A panic while holding the lock left a whole list; it stays usable.
        self.list.lock().unwrap_or_else(|e| e.into_inner())
    }
}

impl Default for MemoryPaused {
    fn default() -> Self {
        Self::new(Vec::new())
    }
}

/// `pause-task`: keeps the record first, then stops the loop and ends the
/// agent and its shells. Places and items stay. Pausing a paused Task again
/// returns its record.
pub(crate) async fn pause<B: SessionBackend, H: Host>(
    backend: &B,
    host: &H,
    terminal: &str,
) -> Result<Paused, Refusal> {
    let agent = match backend.get(&TerminalId::new(terminal.to_owned())).await {
        Ok(agent) => agent,
        Err(e @ BackendError::NotFound(_)) => {
            return find(host, terminal)
                .await
                .ok_or_else(|| (code_of(&e), e.to_string()));
        }
        Err(e) => return Err((code_of(&e), e.to_string())),
    };
    if !is_agent(&agent) {
        let message = format!("{terminal} is not a Task's agent; only a Task pauses");
        return Err((ErrorCode::BadRequest, message));
    }
    let paused = Paused {
        terminal: v2_terminal(&agent),
        paused_at: now(),
        task_loop: active_loop(host, terminal).await,
    };
    host.pause_keep(paused.clone())
        .await
        .map_err(|e| (task_code(&e), e.to_string()))?;
    stop_loop(host, &agent).await;
    if let Err(refusal) = end_agent(backend, &agent).await {
        // The agent still runs, so the Task is not paused.
        if let Err(e) = host.pause_drop(terminal).await {
            tracing::warn!(terminal, error = %e, "a pause that failed stays listed");
        }
        return Err(refusal);
    }
    if let Some(dir) = agent.labels.get(DIR) {
        shells::close_all(backend, dir).await;
    }
    tracing::info!(terminal, "task paused");
    Ok(paused)
}

/// `resume-task`: runs the paused Task's agent again from its record. A
/// muxa agent already running in its working place takes the Task over.
pub(crate) async fn resume<B: SessionBackend, H: Host>(
    backend: &B,
    host: &H,
    inbox: &Inbox,
    id: &str,
) -> Result<(TerminalInfo, bool), Refusal> {
    let paused = find(host, id)
        .await
        .ok_or_else(|| (ErrorCode::NotFound, format!("{id} is not a paused Task")))?;
    let record = info_of(&paused);
    let running = match record.labels.get(DIR) {
        Some(dir) => live_agent_in(backend, dir).await,
        None => None,
    };
    let (started, existing) = match running {
        Some(agent) => (agent, true),
        None => {
            let prepared = host
                .restart(&record)
                .await
                .map_err(|e| (task_code(&e), e.to_string()))?;
            tasks::start(backend, host, prepared)
                .await
                .map_err(|e| (code_of(&e), e.to_string()))?
        }
    };
    tracing::info!(paused = id, terminal = %started.id, existing, "task resumed");
    end_pauses(host, inbox, &started).await;
    Ok((started, existing))
}

/// `close-task` for a paused id: checks and removes its places as for a live
/// Task, then marks its items Done and drops the record.
pub(crate) async fn close<H: Host>(
    host: &H,
    inbox: &Inbox,
    paused: &Paused,
    mode: WorktreeMode,
) -> Result<(), Refusal> {
    let places = places_of(&info_of(paused));
    let refused = |e: tasks::TaskError| (task_code(&e), e.to_string());
    host.check_close(&places, mode).await.map_err(refused)?;
    host.close_places(&places, mode).await.map_err(refused)?;
    let id = paused.terminal.id.as_str();
    host.pause_drop(id).await.map_err(refused)?;
    tracing::info!(terminal = id, places = places.len(), "paused task closed");
    done_all(inbox, id)
}

/// An agent started in a paused Task's working place: the pause ends. Its
/// open items move to `started`, and an active loop is armed again unless
/// the new agent already has one.
pub(crate) async fn end_pauses<H: Host>(host: &H, inbox: &Inbox, started: &TerminalInfo) {
    let Some(dir) = started.labels.get(DIR) else {
        return;
    };
    let new = started.id.as_str();
    for paused in host.paused().await {
        if paused.terminal.labels.get(DIR) != Some(dir) {
            continue;
        }
        let old = paused.terminal.id.as_str();
        let moved = inbox.change(|b, _| {
            b.retarget(old, new);
            Ok(())
        });
        if let Err(e) = moved {
            tracing::warn!(paused = old, terminal = new, error = %e, "items stay on the paused id");
        }
        if let Some(spec) = &paused.task_loop
            && active_loop(host, new).await.is_none()
            && let Err(e) = host.loop_start(started, spec, false).await
        {
            tracing::warn!(terminal = new, error = %e, "the paused Task Loop was not armed again");
        }
        if let Err(e) = host.pause_drop(old).await {
            tracing::warn!(paused = old, error = %e, "an ended pause stays listed");
        }
    }
}

pub(crate) async fn find<H: Host>(host: &H, id: &str) -> Option<Paused> {
    host.paused()
        .await
        .into_iter()
        .find(|p| p.terminal.id == id)
}

async fn active_loop<H: Host>(host: &H, terminal: &str) -> Option<LoopSpec> {
    host.loops()
        .await
        .into_iter()
        .find(|l| l.terminal == terminal && ACTIVE.contains(&l.state.as_str()))
        .map(|l| LoopSpec {
            check: Some(l.check),
            max_attempts: Some(l.max_attempts),
            timeout: Some(l.timeout),
        })
}

async fn live_agent_in<B: SessionBackend>(backend: &B, dir: &str) -> Option<TerminalInfo> {
    match backend.list().await {
        Ok(all) => all
            .into_iter()
            .find(|t| t.live && is_agent(t) && t.labels.get(DIR).map(String::as_str) == Some(dir)),
        Err(e) => {
            tracing::warn!(error = %e, "terminals not listed before a resume");
            None
        }
    }
}

/// The agent as a paused record shows it: not live, no status.
fn v2_terminal(agent: &TerminalInfo) -> muxa_protocol::v2::Terminal {
    let mut t = to_wire(agent.clone());
    t.live = false;
    t.status = None;
    t
}

/// The record as the ended agent terminal it was, for `Host::restart`.
fn info_of(paused: &Paused) -> TerminalInfo {
    let t = &paused.terminal;
    TerminalInfo {
        id: TerminalId::new(t.id.clone()),
        name: t.name.clone(),
        live: false,
        status: None,
        labels: t.labels.clone(),
        size: Size {
            cols: t.cols,
            rows: t.rows,
        },
        exit: None,
    }
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}
