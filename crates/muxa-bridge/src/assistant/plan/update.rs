//! `update-plan` (ADR 0050 §2, §6): the user reorders Today's rows or
//! removes one from Today. Only the newest `resulted` plan changes; `top`
//! keeps the agent's order. Each change records one correction.
use muxa_protocol::{CorrectionKind, ErrorBody, ErrorCode, Plan, PlanAction};

use crate::assistant::corrections::{self, Seen};
use crate::assistant::work::{Fail, check_rev};
use crate::convert::error_body;
use crate::task_records::Config;

pub struct Change {
    pub plan: String,
    pub rev: u64,
    pub action: PlanAction,
    pub order: Vec<String>,
    pub row: Option<String>,
    pub text: Option<String>,
}

/// Blocking. The changed plan as stored. A reorder to the order shown, or
/// a dismiss of a dismissed row, changes nothing and keeps `rev`.
pub fn update(config: &Config, c: Change, now: u64) -> Result<Plan, ErrorBody> {
    if !is_plan_id(&c.plan) {
        return Err(bad("update-plan: plan must be p- and 8 hex digits"));
    }
    let reason = check_fields(&c)?;
    // Read before the plan's lock: the item store has locks of its own.
    let workspace = c
        .row
        .as_deref()
        .and_then(|row| corrections::row_workspace(config, row));
    let shelf = super::shelf(config)?;
    let changed = shelf.update(&c.plan, |p| -> Result<_, Fail> {
        check_current(&shelf, p)?;
        check_rev("Plan", &p.id, p.rev, c.rev)?;
        let seen = match c.action {
            PlanAction::Reorder => reorder(p, &c.order)?,
            PlanAction::Dismiss => {
                let row = c.row.as_deref().unwrap_or_default();
                dismiss(p, row, workspace, reason)?
            }
        };
        if seen.is_some() {
            p.rev += 1;
        }
        Ok((p.clone(), seen))
    })?;
    let (plan, seen) = changed.ok_or_else(|| bad(format!("no plan {}", c.plan)))?;
    tracing::info!(plan = %plan.id, action = ?c.action, rev = plan.rev, "plan updated");
    if let Some(seen) = seen {
        corrections::record(config, &plan, seen, now);
    }
    Ok(plan)
}

/// The fields each action takes, and the dismiss's reason.
fn check_fields(c: &Change) -> Result<Option<String>, ErrorBody> {
    match c.action {
        PlanAction::Reorder if c.row.is_some() || c.text.is_some() => {
            Err(bad("update-plan: reorder takes only order"))
        }
        PlanAction::Reorder if c.order.is_empty() => {
            Err(bad("update-plan: reorder needs order, every row once"))
        }
        PlanAction::Reorder => Ok(None),
        PlanAction::Dismiss if !c.order.is_empty() => {
            Err(bad("update-plan: dismiss takes no order"))
        }
        PlanAction::Dismiss if c.row.is_none() => Err(bad("update-plan: dismiss needs row")),
        PlanAction::Dismiss => corrections::check_reason(c.text.as_deref()),
    }
}

/// Only the plan the user sees as Today changes.
fn check_current(shelf: &crate::assistant::store::Shelf<Plan>, p: &Plan) -> Result<(), Fail> {
    let newest = corrections::newest_resulted(shelf.all()?);
    if newest.is_some_and(|n| n.id == p.id) {
        return Ok(());
    }
    Err(error_body(
        ErrorCode::Conflict,
        format!(
            "plan {} is not the newest resulted plan; list the Inbox again",
            p.id
        ),
    )
    .into())
}

fn rows(p: &Plan) -> Vec<String> {
    p.top
        .iter()
        .filter_map(corrections::row_id)
        .map(str::to_owned)
        .collect()
}

/// `order` is every row of `top` once.
fn reorder(p: &mut Plan, order: &[String]) -> Result<Option<Seen>, Fail> {
    let rows = rows(p);
    let mut sorted = order.to_vec();
    sorted.sort();
    let mut expected = rows.clone();
    expected.sort();
    if sorted != expected {
        return Err(bad(format!(
            "update-plan: order must name each of the plan's {} rows once",
            rows.len()
        ))
        .into());
    }
    let before = if p.user_order.is_empty() {
        rows
    } else {
        p.user_order.clone()
    };
    if before == order {
        return Ok(None);
    }
    p.user_order = order.to_vec();
    Ok(Some(Seen {
        kind: CorrectionKind::Reordered,
        row: None,
        workspace: None,
        before,
        after: order.to_vec(),
        reason: None,
    }))
}

fn dismiss(
    p: &mut Plan,
    row: &str,
    workspace: Option<String>,
    reason: Option<String>,
) -> Result<Option<Seen>, Fail> {
    if !corrections::listed(p, row) {
        return Err(bad(format!("update-plan: {row} is not a row of plan {}", p.id)).into());
    }
    if p.dismissed.iter().any(|d| d == row) {
        return Ok(None);
    }
    p.dismissed.push(row.to_owned());
    Ok(Some(Seen {
        reason,
        ..Seen::row(CorrectionKind::Dismissed, row, workspace)
    }))
}

/// `p-` and 8 lowercase hex digits, as the store makes them.
fn is_plan_id(id: &str) -> bool {
    id.strip_prefix("p-").is_some_and(|h| {
        h.len() == 8
            && h.bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    })
}

fn bad(message: impl Into<String>) -> ErrorBody {
    error_body(ErrorCode::BadRequest, message.into())
}

#[cfg(test)]
#[path = "update_tests.rs"]
mod tests;
