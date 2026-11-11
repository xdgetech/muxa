//! `failure` Inbox items for assistant runs (ADR 0048, addendum 2026-11-11):
//! a request version's step or a plan whose delivery is `failed` makes one
//! item in its run's file, keyed so a second scan makes none. The reason is
//! host text; it never reaches a log.
use std::collections::HashMap;
use std::path::Path;

use muxa_protocol::{AssistantRequest, Delivery, InboxItem, ItemKind, ItemState, Plan, WorkItem};

use super::store::Shelf;
use crate::items::produce::{MAX_QUESTION, one_line};
use crate::items::store::{ItemStore, StoredItem, new_id};
use crate::records::{Reason, Record, Status, Workspaces};
use crate::routines::store::StateError;
use crate::task_records::Config;
use crate::task_records::handoff::trust_reason;

/// Seconds after a failure in which it still makes its item. An older one,
/// as from before this host made them, asks nothing.
const WINDOW: u64 = 24 * 60 * 60;
const PLAN_TITLE: &str = "Overall plan";
const NOT_SENT: &str = "The agent did not become ready within 30 seconds, so its prompt \
                        was not sent.";
const NO_RESULT: &str = "The run ended without a valid result.";

/// The item's key in its run's file.
pub fn request_key(request: &str, version: u64, step: u32) -> String {
    format!("failure:{request}:{version}:{step}")
}

pub fn plan_key(plan: &str) -> String {
    format!("failure:{plan}")
}

/// One item to make.
struct Failure {
    task: String,
    key: String,
    workspace: String,
    title: String,
    reason: String,
}

/// Blocking. Makes the item of every recent failed request and plan
/// that has a run.
pub fn scan(config: &Config, now: u64) {
    let Some(dir) = config.state_dir.as_ref().filter(|d| d.is_dir()) else {
        return;
    };
    let recent = |d: Delivery, at: u64| d == Delivery::Failed && now.saturating_sub(at) <= WINDOW;
    let requests: Vec<AssistantRequest> = read::<AssistantRequest>(dir)
        .into_iter()
        .filter(|q| recent(q.delivery, q.delivery_at) && q.run.is_some())
        .collect();
    let plans: Vec<Plan> = read::<Plan>(dir)
        .into_iter()
        .filter(|p| recent(p.delivery, p.delivery_at) && p.run.is_some())
        .collect();
    if requests.is_empty() && plans.is_empty() {
        return;
    }
    let records: HashMap<String, Record> = config
        .records_if_any()
        .into_iter()
        .filter(Record::is_assistant_run)
        .map(|r| (r.id.clone(), r))
        .collect();
    let workspaces = config.load_workspaces().unwrap_or_default();
    let works = Shelf::<WorkItem>::open(dir);
    let title = |q: &AssistantRequest, r: &Record| {
        works
            .as_ref()
            .ok()
            .and_then(|s| s.get(&q.work).ok().flatten())
            .map_or_else(|| r.title.clone(), |w| w.title)
    };
    let mut failures = Vec::new();
    for q in &requests {
        let Some(r) = q.run.as_ref().and_then(|t| records.get(t)) else {
            continue;
        };
        let Some(run) = r.request_run().filter(|run| run.request == q.id) else {
            continue;
        };
        failures.push(Failure {
            task: r.id.clone(),
            key: request_key(&q.id, run.version, run.step()),
            workspace: q.workspace.clone(),
            title: title(q, r),
            reason: reason(r, None, &workspaces),
        });
    }
    for p in &plans {
        let Some(r) = p.run.as_ref().and_then(|t| records.get(t)) else {
            continue;
        };
        if r.plan_run().is_none_or(|run| run.plan != p.id) {
            continue;
        }
        failures.push(Failure {
            task: r.id.clone(),
            key: plan_key(&p.id),
            workspace: r.workspace.clone(),
            title: PLAN_TITLE.into(),
            reason: reason(r, p.reason.as_deref(), &workspaces),
        });
    }
    let store = match ItemStore::open(dir) {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!(error = %e, "cannot open the items for assistant failures");
            return;
        }
    };
    for f in failures {
        match make(&store, &f, now) {
            Ok(Some(item)) => {
                tracing::info!(task = %f.task, item, key = %f.key, "inbox item made: the assistant run failed")
            }
            Ok(None) => {}
            Err(e) => {
                tracing::warn!(task = %f.task, error = %e, "cannot make an assistant failure item")
            }
        }
    }
}

fn read<T: super::store::Kind>(dir: &Path) -> Vec<T> {
    match Shelf::<T>::open(dir).and_then(|s| s.all()) {
        Ok(all) => all,
        Err(e) => {
            tracing::warn!(error = %e, "cannot read the assistant store for failures");
            Vec::new()
        }
    }
}

/// What the user can act on: the trust dialog first, then the host's own
/// reason (`plan`), then what the run's record says.
fn reason(r: &Record, plan: Option<&str>, workspaces: &Workspaces) -> String {
    let root = workspaces.get(&r.workspace).map(|w| w.root.as_path());
    if let Some(why) = trust_reason(r, root) {
        return why;
    }
    if let Some(why) = plan.filter(|w| !w.is_empty()) {
        return why.to_owned();
    }
    from_record(r)
}

fn from_record(r: &Record) -> String {
    let stopped = r.task_loop().ok().flatten().and_then(|lp| lp.reason);
    let why = match stopped {
        Some(Reason::NotSent) => NOT_SENT,
        Some(Reason::Limit) => {
            "The agent's result did not pass the check, also after the fail prompts."
        }
        Some(Reason::Timeout) => "The result check timed out.",
        Some(Reason::Error) => "The agent stopped with an error.",
        Some(Reason::NoAgent) => "The agent exited before it answered.",
        Some(Reason::Stalled) => "The agent did not take the fail prompt.",
        _ => match r.note.as_deref() {
            Some(note)
                if r.status == Status::Dropped && note.starts_with("The run did not start") =>
            {
                return note.to_owned();
            }
            _ => NO_RESULT,
        },
    };
    why.to_owned()
}

/// Under the file's lock. The new item's id, or None when the key made one.
fn make(store: &ItemStore, f: &Failure, now: u64) -> Result<Option<String>, StateError> {
    store.update::<_, StateError>(&f.task, |items| {
        if items.has_key(&f.key) {
            return Ok(None);
        }
        let item = item_for(f, new_id()?, now);
        let id = item.id.clone();
        items.items.push(StoredItem {
            item,
            key: f.key.clone(),
        });
        Ok(Some(id))
    })
}

/// No Task and no session: the run is not the user's to drop or retry.
fn item_for(f: &Failure, id: String, at: u64) -> InboxItem {
    let question = one_line(&f.reason, MAX_QUESTION);
    let summary = (question != f.reason).then(|| f.reason.clone());
    let mut item = InboxItem {
        memory: None,
        write: None,
        handoff: None,
        id,
        kind: ItemKind::Failure,
        state: ItemState::Open,
        question,
        workspace: f.workspace.clone(),
        title: one_line(&f.title, MAX_QUESTION),
        created_at: at,
        read_at: None,
        later_at: None,
        until: None,
        protected_at: None,
        answered_at: None,
        answer: None,
        summary,
        task: None,
        session: None,
        pane: None,
        prompt: None,
        attachments: Default::default(),
        routine: None,
        proposal: None,
        sort: None,
        delegate: None,
        hypothesis: None,
        answers: Vec::new(),
    };
    item.answers = crate::items::answers(&item);
    item
}

#[cfg(test)]
#[path = "failures_tests.rs"]
mod tests;
