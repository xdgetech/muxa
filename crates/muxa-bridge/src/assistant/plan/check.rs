//! The Loop check of an overall run (ADR 0047 §5): the plan file's own
//! rules, then the rules the host holds it to against the store, then the
//! plan's one write. Reasons go to the run's check log and name fields and
//! indices; host logs get ids and counts only.
use std::collections::HashMap;
use std::path::Path;

use muxa_protocol::{
    Capture, CaptureState, Delivery, InboxItem, ItemState, Plan, WorkItem, WorkState,
};

use super::file::{self, Checked, MAX_TOP, Proposal};
use crate::assistant::record::PlanRun;
use crate::assistant::result::Stored;
use crate::assistant::store::{Extra, Shelf};
use crate::records::{Loop, LoopState, Reason, Record, Workspaces, now};
use crate::routines::store::StateError;
use crate::task_records::Config;

/// The plan file's field for the checked proposals, until slice B5 makes them Inbox items.
pub const PROPOSALS_FIELD: &str = "proposals";

/// What the plan is held to: the store as the check reads it.
pub struct Facts<'a> {
    pub work: &'a [WorkItem],
    pub items: &'a [InboxItem],
    pub captures: &'a [Capture],
    pub workspaces: &'a Workspaces,
}

fn closed(w: &WorkItem) -> bool {
    matches!(w.state, WorkState::Done | WorkState::Withdrawn)
}

/// Every rule of §5 that `checked` breaks against the store.
pub fn against(checked: &Checked, facts: &Facts<'_>) -> Vec<String> {
    let work: HashMap<&str, &WorkItem> = facts.work.iter().map(|w| (w.id.as_str(), w)).collect();
    let open_work = |id: &str| work.get(id).copied().filter(|w| !closed(w));
    let mut reasons = Vec::new();
    for (i, row) in checked.top.iter().enumerate() {
        if let Some(id) = &row.work {
            match open_work(id) {
                None => reasons.push(format!(
                    "top[{i}].work is not an open Work item on this host."
                )),
                Some(w) if w.later_at.is_some() => reasons.push(format!(
                    "top[{i}].work was moved down with Later by the user; leave it out."
                )),
                Some(_) => {}
            }
        }
        if let Some(id) = &row.item {
            let open = facts
                .items
                .iter()
                .any(|it| it.id == *id && it.state == ItemState::Open);
            if !open {
                reasons.push(format!(
                    "top[{i}].item is not an open Inbox item on this host."
                ));
            }
        }
    }
    pinned_first(checked, facts.work, &mut reasons);
    for (i, p) in checked.proposals.iter().enumerate() {
        proposal(i, p, facts, &open_work, &mut reasons);
    }
    reasons
}

/// The pinned open Work items, in pin order, fill the first rows.
fn pinned_first(checked: &Checked, work: &[WorkItem], reasons: &mut Vec<String>) {
    let mut pinned: Vec<&WorkItem> = work
        .iter()
        .filter(|w| !closed(w) && w.pinned_at.is_some())
        .collect();
    pinned.sort_by(|a, b| a.pinned_at.cmp(&b.pinned_at).then(a.id.cmp(&b.id)));
    pinned.truncate(MAX_TOP);
    if checked.top.len() < pinned.len() {
        reasons.push(format!(
            "top must start with the {} pinned Work items, oldest pin first.",
            pinned.len()
        ));
        return;
    }
    for (i, p) in pinned.iter().enumerate() {
        if checked.top[i].work.as_deref() != Some(p.id.as_str()) {
            reasons.push(format!(
                "top[{i}] must be the pinned Work item {}: pinned items fill the first rows, \
                 oldest pin first.",
                p.id
            ));
        }
    }
}

fn proposal<'a>(
    i: usize,
    p: &Proposal,
    facts: &Facts<'_>,
    open_work: &dyn Fn(&str) -> Option<&'a WorkItem>,
    reasons: &mut Vec<String>,
) {
    let at = format!("proposals[{i}]");
    match p {
        Proposal::Sort(s) => {
            let unsorted = facts
                .captures
                .iter()
                .any(|c| c.id == s.capture && c.state == CaptureState::Received);
            if !unsorted {
                reasons.push(format!("{at}.capture is not an unsorted Capture."));
            }
            if s.work.as_deref().is_some_and(|w| open_work(w).is_none()) {
                reasons.push(format!("{at}.work is not an open Work item."));
            }
            if s.workspace
                .as_deref()
                .is_some_and(|w| facts.workspaces.get(w).is_none())
            {
                reasons.push(format!("{at}.workspace is not a workspace on this host."));
            }
        }
        Proposal::Delegate(d) => {
            if open_work(&d.work).is_none() {
                reasons.push(format!("{at}.work is not an open Work item."));
            }
            let asked = facts.workspaces.overall_requests(&d.workspace);
            if asked == muxa_protocol::OverallRequests::None {
                reasons.push(format!(
                    "{at}.workspace is not a workspace you may ask (see the brief)."
                ));
            }
        }
    }
}

/// Why a plan that passed is not stored.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Refused {
    #[error("plan {0} has another run")]
    OtherRun(String),
    #[error("plan {0} failed or was replaced")]
    Ended(String),
}

/// Stores `checked` as `plan`'s answer, from the run `task`, in one write.
pub fn store(
    plan: &mut Plan,
    extra: &mut Extra,
    checked: Checked,
    due_soon: Vec<String>,
    task: &str,
    at: u64,
) -> Result<Stored, Refused> {
    if plan.run.as_deref().is_some_and(|r| r != task) {
        return Err(Refused::OtherRun(plan.id.clone()));
    }
    match plan.delivery {
        Delivery::Resulted => return Ok(Stored::Already),
        Delivery::Failed | Delivery::Cancelled | Delivery::CancelRequested => {
            return Err(Refused::Ended(plan.id.clone()));
        }
        _ => {}
    }
    plan.top = checked.top;
    plan.due_soon = due_soon;
    plan.notes = checked.notes;
    plan.run = Some(task.to_owned());
    plan.delivery = Delivery::Resulted;
    plan.delivery_at = at;
    plan.rev += 1;
    match serde_json::to_value(&checked.proposals) {
        Ok(v) => {
            extra.insert(PROPOSALS_FIELD.into(), v);
        }
        Err(e) => tracing::warn!(plan = %plan.id, error = %e, "cannot keep a plan's proposals"),
    }
    Ok(Stored::New)
}

/// Blocking. Exit 0 once the plan is stored (also when it was already).
pub fn publish(config: &Config, record: &Record, run: &PlanRun) -> Result<Stored, Vec<String>> {
    let one = |r: String| vec![r];
    let dir = record
        .dir
        .as_ref()
        .ok_or_else(|| one("The run has no directory.".into()))?;
    let checked = file::read(dir, run)?;
    let gathered = gather(config).map_err(|e| {
        tracing::warn!(task = %record.id, error = %e, "cannot read the store for a plan");
        one(retry())
    })?;
    let workspaces = config.load_workspaces().map_err(|e| {
        tracing::warn!(task = %record.id, error = %e.message, "cannot load workspaces for a plan");
        one(retry())
    })?;
    let facts = Facts {
        work: &gathered.work,
        items: &gathered.items,
        captures: &gathered.captures,
        workspaces: &workspaces,
    };
    let reasons = against(&checked, &facts);
    if !reasons.is_empty() {
        return Err(reasons);
    }
    let at = now();
    let (rows, proposals) = (checked.top.len(), checked.proposals.len());
    let notes = checked.notes.clone();
    let due_soon = super::derive::due_soon(&gathered.work, at);
    let shelf = super::shelf(config).map_err(|_| one(retry()))?;
    let stored = shelf.update_with_extra::<_, Failure>(&run.plan, |p, extra| {
        store(p, extra, checked, due_soon, &record.id, at).map_err(Failure::Refused)
    });
    match stored {
        Ok(Some(s)) => {
            tracing::info!(task = %record.id, plan = %run.plan, rows, proposals, stored = ?s, "plan stored");
            if let (Stored::New, Some(notes)) = (s, &notes) {
                let rev = shelf.get(&run.plan).ok().flatten().map_or(0, |p| p.rev);
                crate::assistant::apply::auto_apply(config, record, &run.plan, rev, notes);
            }
            Ok(s)
        }
        Ok(None) => Err(one(format!(
            "Plan {} is no longer on this host; the plan is not stored.",
            run.plan
        ))),
        Err(Failure::Refused(r)) => Err(one(format!(
            "This run's plan is not stored: {r}. A newer plan replaced it."
        ))),
        Err(Failure::State(e)) => {
            tracing::warn!(task = %record.id, error = %e, "cannot store a plan");
            Err(one(retry()))
        }
    }
}

fn retry() -> String {
    "muxa could not store the plan. It checks again after your next turn.".into()
}

/// Work items, Inbox items and Captures as the store holds them.
struct Gathered {
    work: Vec<WorkItem>,
    items: Vec<InboxItem>,
    captures: Vec<Capture>,
}

/// Blocking. Errors name paths and ids, never content.
fn gather(config: &Config) -> Result<Gathered, String> {
    let text = |e: muxa_protocol::ErrorBody| e.message;
    let work = crate::assistant::shelf::<WorkItem>(config)
        .map_err(text)?
        .all()
        .map_err(|e| e.to_string())?;
    let captures = crate::assistant::shelf::<Capture>(config)
        .map_err(text)?
        .all()
        .map_err(|e| e.to_string())?;
    let entries = crate::routines::inbox::list(config).map_err(text)?;
    let items = crate::items::list(config, &entries).map_err(text)?;
    Ok(Gathered {
        work,
        items,
        captures,
    })
}

#[derive(Debug, thiserror::Error)]
enum Failure {
    #[error(transparent)]
    Refused(Refused),
    #[error(transparent)]
    State(#[from] StateError),
}

/// Blocking. After the Loop stopped an overall run without a plan, the plan
/// is `failed`. A stop by a status change is a replacement, not a failure.
pub fn stopped(state_dir: &Path, record: &Record, lp: &Loop) {
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
    if lp.state != LoopState::Stopped || !failing {
        return;
    }
    let Some(run) = record.plan_run() else {
        return;
    };
    let failed = Shelf::<Plan>::open(state_dir).and_then(|s| {
        end(
            &s,
            &run.plan,
            Some(&record.id),
            Delivery::Failed,
            "No valid plan.",
            now(),
        )
    });
    match failed {
        Ok(true) => tracing::info!(task = %record.id, plan = %run.plan, "plan failed"),
        Ok(false) => {}
        Err(e) => {
            tracing::warn!(task = %record.id, plan = %run.plan, error = %e, "cannot mark a plan failed")
        }
    }
}

/// Ends `plan` as `state` (`failed`, or `cancelled` when a newer plan
/// replaces it) with `why`, unless it has its answer or another run. True
/// when it changed.
pub fn end(
    plans: &Shelf<Plan>,
    plan: &str,
    task: Option<&str>,
    state: Delivery,
    why: &str,
    at: u64,
) -> Result<bool, StateError> {
    let changed = plans.update_with_extra::<_, StateError>(plan, |p, extra| {
        let other = task.is_some() && p.run.is_some() && p.run.as_deref() != task;
        if other || crate::assistant::delivery::settled(p.delivery) {
            return Ok(false);
        }
        p.delivery = state;
        p.delivery_at = at;
        p.rev += 1;
        extra.insert(super::REASON_FIELD.into(), why.into());
        extra.remove(super::start::CREATING_FIELD);
        Ok(true)
    })?;
    Ok(changed.unwrap_or(false))
}

#[cfg(test)]
#[path = "check_tests.rs"]
mod tests;
