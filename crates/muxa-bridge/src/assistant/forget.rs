//! `forget {record}` (ADR 0048 §5): removes a correction or a hypothesis.
//! Forgetting a correction also removes every `proposed` hypothesis it is
//! evidence of, and their items, so no guess outlives its ground. The next
//! brief is built from live records and the plan check refuses an id that
//! is not live, so a forgotten record cannot reach a later plan.
use std::collections::HashSet;

use muxa_protocol::{Correction, ErrorBody, ErrorCode, Hypothesis, HypothesisState};

use super::hypotheses::{is_id, items};
use super::work::Fail;
use crate::convert::error_body;
use crate::items::store::TaskItems;
use crate::task_records::Config;

/// Blocking. Removes `record`, an `x-` or `h-` id. Errors name ids only.
pub fn forget(config: &Config, record: &str, now: u64) -> Result<(), ErrorBody> {
    if is_id(record, "x-") {
        correction(config, record)
    } else if is_id(record, "h-") {
        hypothesis(config, record, now)
    } else {
        Err(bad(
            "forget: record must be a correction (x- and 8 hex digits) or a hypothesis (h-)",
        ))
    }
}

/// The hypotheses it grounds go first: a crash between leaves the
/// correction, and Forget again finishes.
fn correction(config: &Config, id: &str) -> Result<(), ErrorBody> {
    let shelf = super::shelf::<Correction>(config)?;
    if shelf.get(id).map_err(|e| super::state_error(&e))?.is_none() {
        return Err(bad(format!("no correction {id}")));
    }
    let grounded: Vec<Hypothesis> = super::shelf::<Hypothesis>(config)?
        .all()
        .map_err(|e| super::state_error(&e))?
        .into_iter()
        .filter(|h| h.state == HypothesisState::Proposed && h.evidence.iter().any(|x| x == id))
        .collect();
    let mut removed = HashSet::new();
    for h in grounded {
        if remove_hypothesis(config, &h.id, |s| s == HypothesisState::Proposed)? {
            removed.insert(h.id);
        }
    }
    let items = items::each(config, &removed, remove_items)?;
    shelf.remove(id).map_err(|e| super::state_error(&e))?;
    tracing::info!(
        record = id,
        hypotheses = removed.len(),
        items,
        "correction forgotten"
    );
    Ok(())
}

fn hypothesis(config: &Config, id: &str, now: u64) -> Result<(), ErrorBody> {
    let stored = super::shelf::<Hypothesis>(config)?
        .get(id)
        .map_err(|e| super::state_error(&e))?
        .ok_or_else(|| bad(format!("no hypothesis {id}")))?;
    if stored.state == HypothesisState::Confirmed {
        return Err(confirmed(config, id, now));
    }
    if !remove_hypothesis(config, id, |s| s != HypothesisState::Confirmed)? {
        return Err(confirmed(config, id, now));
    }
    let items = items::each(config, &HashSet::from([id.to_owned()]), remove_items)?;
    tracing::info!(record = id, items, "hypothesis forgotten");
    Ok(())
}

/// Removes hypothesis `id` when its state passes `may`. False when it does
/// not, or when it is gone already.
fn remove_hypothesis(
    config: &Config,
    id: &str,
    may: impl Fn(HypothesisState) -> bool,
) -> Result<bool, ErrorBody> {
    let removed = super::shelf::<Hypothesis>(config)?.remove_if(id, |h| {
        if may(h.state) {
            Ok(())
        } else {
            Err(Fail::Client(bad("state changed")))
        }
    });
    match removed {
        Ok(found) => Ok(found.is_some()),
        Err(Fail::Client(_)) => Ok(false),
        Err(Fail::State(e)) => Err(super::state_error(&e)),
    }
}

/// A confirmed hypothesis is the user's policy now: the user deletes that.
fn confirmed(config: &Config, id: &str, now: u64) -> ErrorBody {
    let policy = super::policies::list(config, now)
        .ok()
        .and_then(|all| all.into_iter().find(|p| p.source == id))
        .map_or_else(|| "a policy".to_owned(), |p| format!("policy {}", p.id));
    bad(format!(
        "hypothesis {id} became {policy}: delete the policy with update-policy"
    ))
}

/// Every item under `keys`, whatever its state: its question carries the text.
fn remove_items(items: &mut TaskItems, keys: &HashSet<String>) -> usize {
    let before = items.items.len();
    items.items.retain(|s| !keys.contains(&s.key));
    before - items.items.len()
}

fn bad(message: impl Into<String>) -> ErrorBody {
    error_body(ErrorCode::BadRequest, message.into())
}

#[cfg(test)]
#[path = "forget_tests.rs"]
mod tests;
