//! Starting an overall run (ADR 0049 §5, §9): `plan-now` or a schedule's
//! occurrence. One plan is made at a time; a plan whose run timed out or
//! whose prompt was interrupted is `cancelled` by the next. Budgets are the
//! overall workspace's and the host's. A scheduled plan past a budget is
//! kept as a `failed` plan with the reason, and the next occurrence runs.
use std::sync::Arc;

use muxa_protocol::{Delivery, ErrorBody, ErrorCode, Plan};
use serde_json::json;

use super::WORKSPACE_FIELD;
use crate::assistant::ask::{CLAIMS, CREATING_LIMIT, budget};
use crate::assistant::delivery::{self, Facts};
use crate::assistant::policy::{self, Delegated, Starting};
use crate::assistant::record::{PlanRun, RunFor};
use crate::assistant::store::{Extra, Shelf};
use crate::convert::error_body;
use crate::records::Record;
use crate::task_records::{Config, Context, NewTask, create};

/// The plan file's claim: the epoch second its run began to be made.
pub const CREATING_FIELD: &str = "creating";
/// The plan file's schedule occurrence, `YYYY-MM-DDTHH:MM`.
const OCCURRENCE_FIELD: &str = "occurrence";

/// What starts a plan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum How {
    /// `plan-now`, asked at `at`.
    Now { at: u64, minutes: Option<u32> },
    /// A schedule's occurrence at `at`.
    Scheduled { at: u64, occurrence: String },
}

enum Claimed {
    Run(Plan, Delegated),
    /// A scheduled plan past a budget: kept `failed`, with no run.
    Skipped(Plan),
}

/// The new plan, and the error when its run did not start.
pub async fn start(cx: &Context, how: How) -> Result<(Plan, Option<ErrorBody>), ErrorBody> {
    let (config, fallback) = (Arc::clone(&cx.records), cx.tasks.command.clone());
    let claimed =
        crate::assistant::blocking(move || claim(&config, &how, &fallback, crate::records::now()))
            .await?;
    let (plan, d) = match claimed {
        Claimed::Skipped(plan) => return Ok((plan, None)),
        Claimed::Run(plan, d) => (plan, d),
    };
    let error = run(cx, &plan, &d).await;
    let (config, id) = (Arc::clone(&cx.records), plan.id.clone());
    let stored = crate::assistant::blocking(move || {
        super::shelf(&config)?
            .get(&id)
            .map_err(|e| crate::assistant::state_error(&e))
    })
    .await;
    Ok((stored.ok().flatten().unwrap_or(plan), error))
}

/// Blocking, under [`CLAIMS`]: the refusals, then the plan with its run claimed.
fn claim(config: &Config, how: &How, fallback: &str, now: u64) -> Result<Claimed, ErrorBody> {
    let d = policy::overall(&config.load_workspaces()?, fallback)?;
    let _one = CLAIMS
        .lock()
        .map_err(|_| error_body(ErrorCode::Internal, "the assistant claim lock broke"))?;
    let shelf = super::shelf(config)?;
    let replacing = match unsettled(config, &shelf, now)? {
        Unsettled::Making(id) => {
            return Err(error_body(
                ErrorCode::Conflict,
                format!("plan {id} is being made; wait for it"),
            ));
        }
        Unsettled::Replaceable(plans) => plans,
    };
    let ids: Vec<&str> = replacing.iter().map(|p| p.id.as_str()).collect();
    if let Err(e) = budget(config, &d, &ids, now) {
        let How::Scheduled { .. } = how else {
            return Err(e);
        };
        let plan = new_plan(&shelf, how, &d, now, Some(&e.message))?;
        tracing::info!(plan = %plan.id, "scheduled plan skipped: a budget is reached");
        prune(&shelf);
        return Ok(Claimed::Skipped(plan));
    }
    for p in &replacing {
        replace(config, &shelf, p, now);
    }
    let plan = new_plan(&shelf, how, &d, now, None)?;
    prune(&shelf);
    Ok(Claimed::Run(plan, d))
}

/// The plans without an answer.
enum Unsettled {
    /// This plan is being made: a new one waits.
    Making(String),
    /// None is being made; these a new plan replaces.
    Replaceable(Vec<Plan>),
}

fn unsettled(config: &Config, shelf: &Shelf<Plan>, now: u64) -> Result<Unsettled, ErrorBody> {
    let plans = shelf.all().map_err(|e| crate::assistant::state_error(&e))?;
    let records = config.records_if_any();
    let timeout = super::overall_timeout(config);
    let mut replacing = Vec::new();
    for p in plans.into_iter().filter(|p| !delivery::settled(p.delivery)) {
        let extra = shelf.get_with_extra(&p.id).ok().flatten().map(|(_, e)| e);
        if making(&p, extra.as_ref(), &records, timeout, now) {
            return Ok(Unsettled::Making(p.id));
        }
        replacing.push(p);
    }
    Ok(Unsettled::Replaceable(replacing))
}

/// Blocking. Whether a plan is being made, for the schedule to wait on;
/// `start` checks again under [`CLAIMS`].
pub fn busy(config: &Config, now: u64) -> bool {
    let Ok(shelf) = super::shelf(config) else {
        return false;
    };
    matches!(unsettled(config, &shelf, now), Ok(Unsettled::Making(_)))
}

/// A plan whose run is being made or is working: a fresh claim, or an open
/// run that is pending, received or processing.
fn making(p: &Plan, extra: Option<&Extra>, records: &[Record], timeout: u64, now: u64) -> bool {
    let Some(task) = &p.run else {
        return extra
            .and_then(claimed_at)
            .is_some_and(|since| now.saturating_sub(since) < CREATING_LIMIT);
    };
    let Some(record) = records.iter().find(|r| r.id == *task) else {
        return false;
    };
    let facts = Facts {
        record: Some(record),
        agent: None,
        timeout,
        now,
    };
    crate::assistant::ask::open(record)
        && matches!(
            delivery::derive_plan(p, &facts).0,
            Delivery::Pending | Delivery::Received | Delivery::Processing
        )
}

fn claimed_at(extra: &Extra) -> Option<u64> {
    extra.get(CREATING_FIELD)?.as_u64()
}

/// Ends a plan the new one replaces: `failed`, and its run's record dropped.
fn replace(config: &Config, shelf: &Shelf<Plan>, p: &Plan, now: u64) {
    let why = "A newer plan replaced it.";
    match super::check::end(shelf, &p.id, None, Delivery::Cancelled, why, now) {
        Ok(_) => tracing::info!(plan = %p.id, "plan replaced"),
        Err(e) => tracing::warn!(plan = %p.id, error = %e, "cannot end a replaced plan"),
    }
    if let Some(task) = &p.run {
        crate::assistant::tick::drop_run(config, task, "A newer plan replaced this run.");
    }
}

fn new_plan(
    shelf: &Shelf<Plan>,
    how: &How,
    d: &Delegated,
    now: u64,
    refused: Option<&str>,
) -> Result<Plan, ErrorBody> {
    let (at, minutes, occurrence) = match how {
        How::Now { at, minutes } => (*at, *minutes, None),
        How::Scheduled { at, occurrence } => (*at, None, Some(occurrence.clone())),
    };
    shelf
        .create_with_extra(|id| {
            let plan = Plan {
                id,
                rev: 1,
                at,
                inputs_at: now,
                delivery: if refused.is_some() {
                    Delivery::Failed
                } else {
                    Delivery::Pending
                },
                delivery_at: now,
                minutes,
                run: None,
                workspaces: Vec::new(),
                top: Vec::new(),
                due_soon: Vec::new(),
                unanswered: Vec::new(),
                stale: Vec::new(),
                changed: Vec::new(),
                user_order: Vec::new(),
                dismissed: Vec::new(),
                notes: None,
                reason: refused.map(super::reason),
            };
            let mut extra = Extra::new();
            extra.insert(WORKSPACE_FIELD.into(), d.workspace.clone().into());
            if let Some(o) = occurrence {
                extra.insert(OCCURRENCE_FIELD.into(), o.into());
            }
            if refused.is_none() {
                extra.insert(CREATING_FIELD.into(), json!(now));
            }
            Ok::<_, crate::routines::store::StateError>((plan, extra))
        })
        .map_err(|e| crate::assistant::state_error(&e))
}

/// Keeps the [`super::KEEP`] newest plans, and every plan still being made.
pub(super) fn prune(shelf: &Shelf<Plan>) {
    let Ok(mut plans) = shelf.all() else { return };
    super::newest_first(&mut plans);
    for p in plans.iter().skip(super::KEEP) {
        if !delivery::settled(p.delivery) {
            continue;
        }
        match shelf.remove(&p.id) {
            Ok(()) => tracing::info!(plan = %p.id, "old plan removed"),
            Err(e) => tracing::warn!(plan = %p.id, error = %e, "cannot remove an old plan"),
        }
    }
}

/// Makes the plan's run, and ends the claim. The error when it did not start.
async fn run(cx: &Context, plan: &Plan, d: &Delegated) -> Option<ErrorBody> {
    let (config, workspace) = (Arc::clone(&cx.records), d.workspace.clone());
    // An invalid notes file fails the start before any record or session (ADR 0049 §1).
    let base = crate::assistant::blocking(move || {
        let root = crate::assistant::apply::workspace_root(&config, &workspace)?;
        crate::assistant::notes::read(&root)
            .map(|n| n.base)
            .map_err(|e| crate::assistant::apply::refused(&e))
    })
    .await;
    let (record, error) = match base {
        Err(e) => (None, Some(e)),
        Ok(notes_base) => {
            tracing::info!(plan = %plan.id, workspace = %d.workspace, "overall run starting");
            let new = NewTask {
                workspace: d.workspace.clone(),
                worker: d.worker.clone(),
                // Ids only, as a request's run (ADR 0048 §11).
                title: format!("plan {}", plan.id),
                goal: super::brief::goal(&plan.id),
                assistant: Some(RunFor::Plan(PlanRun {
                    plan: plan.id.clone(),
                    notes_base,
                })),
                ..NewTask::default()
            };
            match cx.env().await {
                Ok(env) => create(&env, new).await,
                Err(e) => (None, Some(e)),
            }
        }
    };
    let config = Arc::clone(&cx.records);
    let (task, why) = (
        record.map(|r| r.id),
        error.as_ref().map(|e| e.message.clone()),
    );
    let id = plan.id.clone();
    let finished = crate::assistant::blocking(move || finish(&config, &id, task, why)).await;
    if let Err(e) = finished {
        tracing::warn!(plan = %plan.id, error = %e.message, "cannot record an overall run");
    }
    error
}

/// Blocking. Ends the claim; links the run, or marks the plan `failed`.
fn finish(
    config: &Config,
    id: &str,
    task: Option<String>,
    why: Option<String>,
) -> Result<(), ErrorBody> {
    if let (Some(task), Some(why)) = (&task, &why) {
        crate::assistant::tick::drop_run(config, task, &format!("The run did not start: {why}"));
    }
    let at = crate::records::now();
    super::shelf(config)?
        .update_with_extra(
            id,
            |p, extra| -> Result<(), crate::routines::store::StateError> {
                extra.remove(CREATING_FIELD);
                if task.is_some() && p.run.is_none() {
                    p.run = task.clone();
                }
                if let (Some(why), false) = (&why, delivery::settled(p.delivery)) {
                    p.delivery = Delivery::Failed;
                    p.delivery_at = at;
                    p.reason = Some(super::reason(&format!("The run did not start: {why}")));
                }
                p.rev += 1;
                Ok(())
            },
        )
        .map_err(|e| crate::assistant::state_error(&e))?;
    match &why {
        Some(_) => tracing::warn!(plan = id, task, "overall run did not start"),
        None => tracing::info!(plan = id, task, "overall run started"),
    }
    Ok(())
}

/// Blocking. Plans whose run is being made, for the budgets.
pub fn starting(config: &Config, at: u64) -> Result<Vec<Starting>, ErrorBody> {
    if !config.state_dir.as_ref().is_some_and(|d| d.is_dir()) {
        return Ok(Vec::new());
    }
    let shelf = super::shelf(config)?;
    let mut out = Vec::new();
    for p in shelf
        .all()
        .map_err(|e| crate::assistant::state_error(&e))?
        .into_iter()
        .filter(|p| p.run.is_none() && !delivery::settled(p.delivery))
    {
        let Ok(Some((p, extra))) = shelf.get_with_extra(&p.id) else {
            continue;
        };
        let (Some(since), Some(workspace)) = (
            claimed_at(&extra),
            extra.get(WORKSPACE_FIELD).and_then(|w| w.as_str()),
        ) else {
            continue;
        };
        if at.saturating_sub(since) < CREATING_LIMIT {
            out.push(Starting {
                request: p.id,
                workspace: workspace.to_owned(),
                since,
            });
        }
    }
    Ok(out)
}
