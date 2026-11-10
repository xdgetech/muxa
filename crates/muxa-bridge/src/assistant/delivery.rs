//! A request's delivery state comes from its run record and the adapter
//! (ADR 0048 §6), never from the agent's words. The stored value is a cache:
//! a list derives it again from the record, the daemon's tick also from the
//! adapter, and only the tick writes it.
use std::collections::HashMap;

use muxa_protocol::{AssistantRequest, Delivery, Plan};

use crate::records::assistant::DEFAULT_RUN_TIMEOUT;
use crate::records::lifecycle::Phase;
use crate::records::{LoopState, Reason, Record, Status, Workspaces};
use crate::task_records::Config;

/// Seconds a prompt send may hold its delivery claim before it counts as
/// interrupted. A send takes milliseconds.
const SEND_GRACE: u64 = 10;

/// What the adapter says about the run's agent. Only the daemon's tick knows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Agent {
    /// Busy since this epoch second.
    Busy(u64),
    NotBusy,
}

/// The facts one request's delivery comes from.
#[derive(Debug, Clone, Copy)]
pub struct Facts<'a> {
    /// The current version's run record, when it exists.
    pub record: Option<&'a Record>,
    pub agent: Option<Agent>,
    /// Seconds: the workspace's `run_timeout`.
    pub timeout: u64,
    pub now: u64,
}

/// No longer derived: a later fact cannot change them. A late result still
/// turns `timed-out` into `resulted`, in the result's own write.
pub fn settled(d: Delivery) -> bool {
    matches!(
        d,
        Delivery::Resulted | Delivery::Failed | Delivery::Cancelled | Delivery::CancelRequested
    )
}

/// The delivery and its time. The time is kept while the state stays.
pub fn derive(q: &AssistantRequest, f: &Facts<'_>) -> (Delivery, u64) {
    if settled(q.delivery) {
        return (q.delivery, q.delivery_at);
    }
    let step = super::steps::current(q);
    let answered = q
        .results
        .iter()
        .find(|r| r.version == q.version && r.step.unwrap_or(1) == step);
    if let Some(r) = answered {
        return (Delivery::Resulted, r.at);
    }
    let Some(record) = f.record.filter(|r| {
        r.request_run().is_some_and(|run| {
            run.request == q.id && run.version == q.version && run.step() == step
        })
    }) else {
        return (q.delivery, q.delivery_at);
    };
    keep_time(
        (q.delivery, q.delivery_at),
        from_record((q.delivery, q.delivery_at), record, f),
    )
}

/// A plan's delivery, as a request's (ADR 0049 §5): only its own run counts.
pub fn derive_plan(p: &Plan, f: &Facts<'_>) -> (Delivery, u64) {
    let seen = (p.delivery, p.delivery_at);
    if settled(p.delivery) {
        return seen;
    }
    let Some(record) = f
        .record
        .filter(|r| r.plan_run().is_some_and(|run| run.plan == p.id))
    else {
        return seen;
    };
    keep_time(seen, from_record(seen, record, f))
}

/// The time is kept while the state stays.
fn keep_time(seen: (Delivery, u64), new: (Delivery, u64)) -> (Delivery, u64) {
    if new.0 == seen.0 { seen } else { new }
}

/// `seen` is the stored delivery and its time.
fn from_record(seen: (Delivery, u64), r: &Record, f: &Facts<'_>) -> (Delivery, u64) {
    match r.restart_operation() {
        Ok(None) => {}
        Ok(Some(op)) if op.phase == Phase::Delivering => {
            if f.now.saturating_sub(op.since) < SEND_GRACE {
                return seen;
            }
            return (Delivery::Unconfirmed, op.since);
        }
        // A restart or an unreadable operation: nothing is typed until recovery.
        Ok(Some(op)) => return (Delivery::Unconfirmed, op.since),
        Err(_) => return (Delivery::Unconfirmed, f.now),
    }
    let interrupted = r
        .open_run()
        .and_then(|run| run.extra.get("outcome"))
        .is_some_and(|v| v == "unconfirmed");
    if interrupted {
        return (Delivery::Unconfirmed, f.now);
    }
    if let Ok(Some(lp)) = r.task_loop() {
        let failing = matches!(
            lp.reason,
            Some(
                Reason::Limit
                    | Reason::Timeout
                    | Reason::Error
                    | Reason::NoAgent
                    | Reason::NotSent
                    | Reason::Stalled
            )
        );
        if lp.state == LoopState::Stopped && failing {
            return (Delivery::Failed, lp.since);
        }
    }
    if r.status == Status::Dropped {
        return (Delivery::Failed, r.updated_at);
    }
    // No agent yet: the launch is still going on.
    if r.pending_prompt().is_some() || r.runs.is_empty() {
        return (Delivery::Pending, r.created_at);
    }
    let started = r.runs.first().map_or(r.created_at, |run| run.started_at);
    let deadline = started.saturating_add(f.timeout);
    if f.now >= deadline {
        return (Delivery::TimedOut, deadline);
    }
    match f.agent {
        Some(Agent::Busy(since)) => (Delivery::Processing, since),
        Some(Agent::NotBusy) => (Delivery::Received, f.now),
        None if matches!(seen.0, Delivery::Received | Delivery::Processing) => seen,
        None => (Delivery::Received, f.now),
    }
}

/// The workspace's `run_timeout`, or the default when it no longer delegates.
pub fn timeout(workspaces: &Workspaces, workspace: &str) -> u64 {
    workspaces
        .get(workspace)
        .and_then(|w| w.assistant.as_ref())
        .map_or(DEFAULT_RUN_TIMEOUT, |d| d.run_timeout)
}

/// Blocking. Sets each request's delivery as a list shows it.
pub fn refresh(config: &Config, requests: &mut [AssistantRequest], now: u64) {
    if requests.iter().all(|q| settled(q.delivery)) {
        requests.iter_mut().for_each(super::steps::sync);
        return;
    }
    let records: HashMap<String, Record> = config
        .records_if_any()
        .into_iter()
        .filter(Record::is_assistant_run)
        .map(|r| (r.id.clone(), r))
        .collect();
    let workspaces = config.load_workspaces().unwrap_or_default();
    for q in requests.iter_mut() {
        let facts = Facts {
            record: q.run.as_ref().and_then(|id| records.get(id)),
            agent: None,
            timeout: timeout(&workspaces, &q.workspace),
            now,
        };
        (q.delivery, q.delivery_at) = derive(q, &facts);
        super::steps::sync(q);
    }
}

#[cfg(test)]
#[path = "delivery_tests.rs"]
mod tests;
