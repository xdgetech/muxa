//! What the host says about a plan, never the agent (ADR 0049 §5): the
//! Later items due soon, stored with the plan; and `unanswered`, `stale`
//! and `changed`, derived each time plans are listed.
use std::collections::HashMap;

use muxa_protocol::{
    AssistantRequest, ChangeKind, Delivery, Plan, PlanChange, PlanRow, WorkItem, WorkState,
};

/// Seconds: "due soon" is within a day.
const DUE_SOON: u64 = 24 * 60 * 60;

fn closed(w: &WorkItem) -> bool {
    matches!(w.state, WorkState::Done | WorkState::Withdrawn)
}

/// Open Work items moved down with Later whose `due` is past or within the
/// next 24 hours, soonest first.
pub fn due_soon(work: &[WorkItem], now: u64) -> Vec<String> {
    let mut soon: Vec<(u64, &str)> = work
        .iter()
        .filter(|w| !closed(w) && w.later_at.is_some())
        .filter_map(|w| Some((w.due.as_ref()?.at, w.id.as_str())))
        .filter(|(at, _)| *at <= now.saturating_add(DUE_SOON))
        .collect();
    soon.sort();
    soon.into_iter().map(|(_, id)| id.to_owned()).collect()
}

/// The requests of open Work items: their current version has no result
/// yet, or their newest result is older than the Work item's last change.
/// A cancelled version waits for nobody.
pub fn unanswered_and_stale(
    work: &[WorkItem],
    requests: &[AssistantRequest],
) -> (Vec<String>, Vec<String>) {
    let open: HashMap<&str, &WorkItem> = work
        .iter()
        .filter(|w| !closed(w))
        .map(|w| (w.id.as_str(), w))
        .collect();
    let (mut unanswered, mut stale) = (Vec::new(), Vec::new());
    for q in requests {
        let Some(w) = open.get(q.work.as_str()) else {
            continue;
        };
        let cancelled = matches!(q.delivery, Delivery::CancelRequested | Delivery::Cancelled);
        if !cancelled && !q.results.iter().any(|r| r.version == q.version) {
            unanswered.push(q.id.clone());
        }
        let newest = q.results.iter().map(|r| r.at).max();
        if newest.is_some_and(|at| at < w.updated_at) {
            stale.push(q.id.clone());
        }
    }
    (unanswered, stale)
}

fn row_id(r: &PlanRow) -> Option<&str> {
    r.work.as_deref().or(r.item.as_deref())
}

/// The rows that entered or moved since `previous`, in this plan's order,
/// then the ones that left, in the previous order. Without a previous plan
/// every row entered.
pub fn changed(plan: &Plan, previous: Option<&Plan>) -> Vec<PlanChange> {
    let before: Vec<&str> = previous
        .map(|p| p.top.iter().filter_map(row_id).collect())
        .unwrap_or_default();
    let now: Vec<&str> = plan.top.iter().filter_map(row_id).collect();
    let mut out = Vec::new();
    for (i, id) in now.iter().enumerate() {
        let change = match before.iter().position(|b| b == id) {
            None => ChangeKind::Entered,
            Some(j) if j != i => ChangeKind::Moved,
            Some(_) => continue,
        };
        out.push(PlanChange {
            id: (*id).to_owned(),
            change,
        });
    }
    out.extend(
        before
            .iter()
            .filter(|b| !now.contains(b))
            .map(|id| PlanChange {
                id: (*id).to_owned(),
                change: ChangeKind::Left,
            }),
    );
    out
}

/// Sets the derived marks on `plans`, newest first: `changed` on each
/// resulted plan against the next older resulted one, and `unanswered` and
/// `stale` on the newest resulted plan only, since they describe now.
pub fn marks(plans: &mut [Plan], work: &[WorkItem], requests: &[AssistantRequest]) {
    let resulted: Vec<usize> = (0..plans.len())
        .filter(|&i| plans[i].delivery == Delivery::Resulted)
        .collect();
    for (k, &i) in resulted.iter().enumerate() {
        let previous = resulted.get(k + 1).map(|&j| plans[j].clone());
        plans[i].changed = changed(&plans[i], previous.as_ref());
    }
    if let Some(&newest) = resulted.first() {
        let (unanswered, stale) = unanswered_and_stale(work, requests);
        plans[newest].unanswered = unanswered;
        plans[newest].stale = stale;
    }
}

#[cfg(test)]
#[path = "derive_tests.rs"]
mod tests;
