//! The overall assistant's plans (ADR 0047 §5): one overall run answers one
//! plan, on a schedule or on demand. The host checks the plan file, stores
//! it, keeps [`KEEP`] plans and derives the marks the agent may not write.
//! A plan never changes a Work item, an Inbox item, their order or `laterAt`.
pub mod brief;
pub mod check;
mod derive;
pub mod file;
pub mod schedule;
pub mod start;
pub(super) mod tick;

use std::collections::HashMap;
use std::sync::Arc;

use muxa_protocol::{AssistantRequest, ErrorBody, ErrorCode, Message, Plan, WorkItem};

use super::delivery::{self, Facts};
use super::store::Shelf;
use crate::convert::error_body;
use crate::records::Record;
use crate::task_records::{Config, Context};

/// Plans kept, newest first.
pub const KEEP: usize = 7;
/// The plan file's reason for a failed plan: host text, never the agent's.
pub const REASON_FIELD: &str = "reason";
/// The plan file's overall workspace, whose notes the plan proposes to change.
pub const WORKSPACE_FIELD: &str = "workspace";
/// Minutes the user may say they have.
const MAX_MINUTES: u32 = 1440;

/// `plan-now {minutes?}`: a `plan-saved` with the new plan, and the error
/// when its run did not start.
pub async fn now(cx: &Context, id: u64, minutes: Option<u32>) -> (Vec<Message>, Option<ErrorBody>) {
    if minutes.is_some_and(|m| m == 0 || m > MAX_MINUTES) {
        return (
            Vec::new(),
            Some(error_body(
                ErrorCode::BadRequest,
                format!("plan-now: minutes must be 1 to {MAX_MINUTES}"),
            )),
        );
    }
    let how = start::How::Now {
        at: crate::records::now(),
        minutes,
    };
    let (plan, error) = match start::start(cx, how).await {
        Ok(started) => started,
        Err(e) => return (Vec::new(), Some(e)),
    };
    let (config, fallback) = (Arc::clone(&cx.records), plan.clone());
    let entry = super::blocking(move || Ok(listed(&config, plan)))
        .await
        .unwrap_or(fallback);
    (
        vec![Message::PlanSaved {
            plan: id,
            entry: Box::new(entry),
        }],
        error,
    )
}

/// Blocking. The plan as `plans[]` lists it; as stored when the list fails.
pub fn listed(config: &Config, plan: Plan) -> Plan {
    let listing = super::list(config).unwrap_or_default();
    list(config, &listing.work, &listing.requests)
        .ok()
        .and_then(|all| all.into_iter().find(|p| p.id == plan.id))
        .unwrap_or(plan)
}

/// Blocking. The kept plans, newest first, with their delivery and marks
/// derived. `work` and `requests` are what `inbox-list` lists.
pub fn list(
    config: &Config,
    work: &[WorkItem],
    requests: &[AssistantRequest],
) -> Result<Vec<Plan>, ErrorBody> {
    if !config.state_dir.as_ref().is_some_and(|d| d.is_dir()) {
        return Ok(Vec::new());
    }
    let mut plans = shelf(config)?.all().map_err(|e| super::state_error(&e))?;
    newest_first(&mut plans);
    refresh(config, &mut plans, crate::records::now());
    derive::marks(&mut plans, work, requests);
    Ok(plans)
}

/// Newest first: by when the inputs were read, then by schedule time. A
/// catch-up made late is newer than a plan-now made before it.
pub fn newest_first(plans: &mut [Plan]) {
    plans.sort_by(|a, b| (b.inputs_at, b.at, &b.id).cmp(&(a.inputs_at, a.at, &a.id)));
}

/// Blocking. Sets each plan's delivery as a list shows it.
fn refresh(config: &Config, plans: &mut [Plan], now: u64) {
    if plans.iter().all(|p| delivery::settled(p.delivery)) {
        return;
    }
    let records: HashMap<String, Record> = config
        .records_if_any()
        .into_iter()
        .filter(|r| r.plan_run().is_some())
        .map(|r| (r.id.clone(), r))
        .collect();
    let timeout = overall_timeout(config);
    for p in plans.iter_mut() {
        let facts = Facts {
            record: p.run.as_ref().and_then(|id| records.get(id)),
            agent: None,
            timeout,
            now,
        };
        (p.delivery, p.delivery_at) = delivery::derive_plan(p, &facts);
    }
}

/// Seconds: the overall workspace's `run_timeout`.
pub fn overall_timeout(config: &Config) -> u64 {
    config
        .load_workspaces()
        .ok()
        .and_then(|w| w.overall()?.assistant.as_ref().map(|d| d.run_timeout))
        .unwrap_or(crate::records::assistant::OVERALL_RUN_TIMEOUT)
}

/// The plans' folder; refused without a state directory.
pub fn shelf(config: &Config) -> Result<Shelf<Plan>, ErrorBody> {
    super::shelf::<Plan>(config)
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
