//! Today refills itself (ADR 0055 §3): when every row of the newest
//! `resulted` plan is closed or "not today", the daemon starts one plan, as
//! `plan-now` does. One refill per emptied plan; the budgets still hold.
use std::sync::Arc;

use muxa_protocol::{Delivery, Plan, WorkItem};

use super::start::How;
use crate::task_records::{Config, Context};

/// The emptied plan's extra field: the epoch second its refill was claimed.
pub const REFILLED_FIELD: &str = "refilled";

/// The daemon's tick: starts the refill in the background when one is due.
pub async fn tick(cx: &Context) {
    let config = Arc::clone(&cx.records);
    let now = crate::records::now();
    let due = tokio::task::spawn_blocking(move || due(&config, now))
        .await
        .unwrap_or_else(|e| {
            tracing::warn!(error = %e, "the Today refill check stopped");
            None
        });
    let Some(after) = due else { return };
    let cx2 = cx.clone();
    cx.pending.spawn(async move {
        let how = How::Refill {
            at: crate::records::now(),
            after: after.clone(),
        };
        match super::start::start(&cx2, how).await {
            Ok((plan, None)) => {
                tracing::info!(plan = %plan.id, after = %after, delivery = ?plan.delivery, "Today refill plan")
            }
            Ok((plan, Some(e))) => {
                tracing::info!(plan = %plan.id, after = %after, error = %e.message, "Today refill plan did not start")
            }
            // Another tick claimed it first, or a plan is being made.
            Err(e) => tracing::debug!(after = %after, error = %e.message, "Today refill not made"),
        }
    });
}

/// Blocking. The id of the newest `resulted` plan when it is emptied and not
/// refilled yet, and no plan run is being made or still open.
pub fn due(config: &Config, now: u64) -> Option<String> {
    config.state_dir.as_ref().filter(|d| d.is_dir())?;
    let workspaces = config.load_workspaces().ok()?;
    workspaces.overall()?.assistant.as_ref()?;
    let shelf = super::shelf(config).ok()?;
    let mut plans = shelf.all().ok()?;
    super::newest_first(&mut plans);
    let newest = plans
        .into_iter()
        .find(|p| p.delivery == Delivery::Resulted)?;
    if newest.top.is_empty() {
        return None;
    }
    let (_, extra) = shelf.get_with_extra(&newest.id).ok()??;
    if extra.contains_key(REFILLED_FIELD) || super::start::busy(config, now) || run_open(config) {
        return None;
    }
    let mut plan = [newest];
    targets(config, &mut plan)?;
    let [plan] = plan;
    emptied(&plan).then_some(plan.id)
}

/// An earlier plan run still holds its slot until its session closes. Waiting
/// for it keeps `max_open_runs` from failing the one refill.
fn run_open(config: &Config) -> bool {
    config
        .records_if_any()
        .iter()
        .any(|r| r.plan_run().is_some() && crate::assistant::ask::open(r))
}

/// Blocking. Derives the rows' targets, reading the Inbox items only when a
/// shown Work item row is not still open. None when the plan is not emptied
/// or a store cannot be read.
fn targets(config: &Config, plan: &mut [Plan; 1]) -> Option<()> {
    let works = crate::assistant::shelf::<WorkItem>(config).ok()?;
    let mut work = Vec::new();
    for row in plan[0].top.iter().filter(|r| !dismissed(&plan[0], r)) {
        let Some(id) = &row.work else { continue };
        if let Some(w) = works.get(id).ok()? {
            work.push(w);
        }
    }
    super::derive::targets(plan, &work, &[]);
    let open_work = plan[0]
        .top
        .iter()
        .any(|r| r.work.is_some() && r.target_state.is_none() && !dismissed(&plan[0], r));
    if open_work {
        return None;
    }
    let shown_item = plan[0]
        .top
        .iter()
        .any(|r| r.item.is_some() && !dismissed(&plan[0], r));
    if shown_item {
        let entries = crate::routines::inbox::list(config).ok()?;
        let items = crate::items::all(config, &entries).ok()?;
        super::derive::targets(plan, &work, &items);
    }
    Some(())
}

fn dismissed(plan: &Plan, row: &muxa_protocol::PlanRow) -> bool {
    row.work
        .as_ref()
        .or(row.item.as_ref())
        .is_some_and(|id| plan.dismissed.contains(id))
}

/// Every row of `plan` is closed or "not today", and it had rows. The rows'
/// `targetState` must be derived.
pub fn emptied(plan: &Plan) -> bool {
    !plan.top.is_empty()
        && plan
            .top
            .iter()
            .all(|r| r.target_state.is_some() || dismissed(plan, r))
}

#[cfg(test)]
#[path = "refill_tests.rs"]
mod tests;
