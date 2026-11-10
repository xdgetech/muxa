//! The overall assistant's hypotheses (ADR 0050 §3): guesses at the user's
//! preferences that a plan proposes and the user confirms or rejects. The
//! host stores them as `proposed`, with limits and an end. Texts never reach
//! a log or an error message.
pub mod answer;
pub mod items;

use std::collections::HashSet;

use muxa_protocol::{Confidence, ErrorBody, Hypothesis, HypothesisState, Policy};

use super::policies::expired;
use super::work::Fail;
use crate::routines::store::StateError;
use crate::task_records::Config;

/// Hypotheses one plan may propose.
pub const MAX_PER_PLAN: usize = 3;
/// `proposed` hypotheses at once (ADR 0050 §8).
pub const MAX_PROPOSED: usize = 10;
/// Seconds a `proposed` hypothesis waits for an answer: 14 days.
pub const PROPOSED_FOR: u64 = 14 * 86_400;
/// Seconds a `rejected` one stays in the brief, and an `expired` one is
/// kept for Forget after its end: 90 days.
pub const REJECTED_FOR: u64 = 90 * 86_400;

/// A hypothesis as a plan file proposes it, checked on its own.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewHypothesis {
    /// Never logged.
    pub text: String,
    pub scope: String,
    /// Correction ids.
    pub evidence: Vec<String>,
    pub confidence: Confidence,
}

/// `proposed` and not past its 14 days.
pub fn open(h: &Hypothesis, now: u64) -> bool {
    h.state == HypothesisState::Proposed && h.expires_at > now
}

/// `rejected` and still in the brief.
fn rejected(h: &Hypothesis, now: u64) -> bool {
    h.state == HypothesisState::Rejected && h.expires_at > now
}

/// The hypotheses a plan may still propose: at most [`MAX_PER_PLAN`], and
/// only as many as [`MAX_PROPOSED`] leaves.
pub fn slots(all: &[Hypothesis], now: u64) -> usize {
    let open = all.iter().filter(|h| open(h, now)).count();
    MAX_PROPOSED.saturating_sub(open).min(MAX_PER_PLAN)
}

/// What "the same text" means: case, spacing and a closing period aside.
pub fn normalized(text: &str) -> String {
    let words: Vec<String> = text.split_whitespace().map(str::to_lowercase).collect();
    words
        .join(" ")
        .trim_end_matches(['.', '!', '。'])
        .trim_end()
        .to_owned()
}

/// `prefix` and 8 lowercase hex digits, as the store makes ids.
pub fn is_id(id: &str, prefix: &str) -> bool {
    id.strip_prefix(prefix).is_some_and(|h| {
        h.len() == 8
            && h.bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    })
}

/// Blocking. Stores a plan's checked hypotheses as `proposed`. One that
/// repeats an active policy, an open or a rejected hypothesis, or cites a
/// correction forgotten since the check, is left out, as is one past
/// [`MAX_PROPOSED`]. Failures are logged; a lost hypothesis is a lost guess.
pub fn adopt(config: &Config, plan: &str, new: Vec<NewHypothesis>, now: u64) -> Vec<Hypothesis> {
    if new.is_empty() {
        return Vec::new();
    }
    match store(config, plan, new, now) {
        Ok(made) => made,
        Err(e) => {
            tracing::warn!(plan, error = %e.message, "cannot store a plan's hypotheses");
            Vec::new()
        }
    }
}

fn store(
    config: &Config,
    plan: &str,
    new: Vec<NewHypothesis>,
    now: u64,
) -> Result<Vec<Hypothesis>, ErrorBody> {
    let shelf = super::shelf::<Hypothesis>(config)?;
    let _folder = shelf.lock_folder().map_err(|e| super::state_error(&e))?;
    let all = shelf.all().map_err(|e| super::state_error(&e))?;
    let policies = super::policies::list(config, now)?;
    let corrections: HashSet<String> = super::corrections::live(config, now)?
        .into_iter()
        .map(|c| c.id)
        .collect();
    let mut taken = taken(&policies, &all, now);
    let mut free = MAX_PROPOSED.saturating_sub(all.iter().filter(|h| open(h, now)).count());
    let mut made = Vec::new();
    for (i, h) in new.into_iter().enumerate() {
        if free == 0 {
            tracing::info!(plan, index = i, "no slot left for a hypothesis");
            break;
        }
        if !taken.insert(normalized(&h.text)) {
            tracing::info!(
                plan,
                index = i,
                "hypothesis repeats a policy or a hypothesis"
            );
            continue;
        }
        if h.evidence.iter().any(|x| !corrections.contains(x)) {
            tracing::info!(plan, index = i, "hypothesis cites a forgotten correction");
            continue;
        }
        let stored = shelf
            .create(|id| {
                Ok::<_, StateError>(Hypothesis {
                    id,
                    text: h.text,
                    scope: h.scope,
                    evidence: h.evidence,
                    confidence: h.confidence,
                    state: HypothesisState::Proposed,
                    plan: plan.to_owned(),
                    expires_at: now + PROPOSED_FOR,
                    at: now,
                })
            })
            .map_err(|e| super::state_error(&e))?;
        tracing::info!(plan, hypothesis = %stored.id, scope = %stored.scope, "hypothesis proposed");
        free -= 1;
        made.push(stored);
    }
    Ok(made)
}

/// Normalized texts a new hypothesis may not repeat.
fn taken(policies: &[Policy], all: &[Hypothesis], now: u64) -> HashSet<String> {
    let policies = policies
        .iter()
        .filter(|p| !expired(p, now))
        .map(|p| normalized(&p.text));
    let hypotheses = all
        .iter()
        .filter(|h| open(h, now) || rejected(h, now))
        .map(|h| normalized(&h.text));
    policies.chain(hypotheses).collect()
}

/// Blocking. What `inbox-list` carries: `proposed` and `rejected`
/// hypotheses still in the brief, newest first.
pub fn list(config: &Config, now: u64) -> Result<Vec<Hypothesis>, ErrorBody> {
    if !config.state_dir.as_ref().is_some_and(|d| d.is_dir()) {
        return Ok(Vec::new());
    }
    let mut all: Vec<Hypothesis> = super::shelf::<Hypothesis>(config)?
        .all()
        .map_err(|e| super::state_error(&e))?
        .into_iter()
        .filter(|h| open(h, now) || rejected(h, now))
        .collect();
    all.sort_by(|a, b| (b.at, &b.id).cmp(&(a.at, &a.id)));
    Ok(all)
}

/// Blocking. The daemon's tick: ends what is due, then makes the newest
/// plan's items.
pub fn tick(config: &Config, now: u64) {
    if !config.state_dir.as_ref().is_some_and(|d| d.is_dir()) {
        return;
    }
    expire(config, now);
    items::publish(config, now);
}

/// A `proposed` one past its 14 days is `expired` and its open item
/// resolved; a `rejected` one leaves after its 90 days, an `expired` one 90
/// days after it ended. Failures are logged and tried again next tick.
fn expire(config: &Config, now: u64) {
    let shelf = match super::shelf::<Hypothesis>(config) {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!(error = %e.message, "cannot open the hypotheses");
            return;
        }
    };
    let Ok(all) = shelf.all() else { return };
    let mut ended = HashSet::new();
    for h in all {
        let due = match h.state {
            HypothesisState::Proposed => h.expires_at <= now,
            HypothesisState::Rejected => h.expires_at <= now,
            HypothesisState::Expired => h.expires_at.saturating_add(REJECTED_FOR) <= now,
            _ => false,
        };
        if !due {
            continue;
        }
        if h.state == HypothesisState::Proposed {
            let expired = shelf.update::<_, StateError>(&h.id, |stored| {
                let still = stored.state == HypothesisState::Proposed && stored.expires_at <= now;
                if still {
                    stored.state = HypothesisState::Expired;
                }
                Ok(still)
            });
            match expired {
                Ok(Some(true)) => {
                    tracing::info!(hypothesis = %h.id, "hypothesis expired unanswered");
                    ended.insert(h.id);
                }
                Ok(_) => {}
                Err(e) => {
                    tracing::warn!(hypothesis = %h.id, error = %e, "cannot expire a hypothesis")
                }
            }
            continue;
        }
        // An answer since the listing may have changed it.
        let state = h.state;
        let removed = shelf.remove_if(&h.id, |stored| {
            if stored.state == state {
                Ok(())
            } else {
                Err(Fail::Client(crate::convert::error_body(
                    muxa_protocol::ErrorCode::Conflict,
                    "changed",
                )))
            }
        });
        match removed {
            Ok(Some(_)) => {
                tracing::info!(hypothesis = %h.id, state = ?state, "old hypothesis removed")
            }
            Ok(None) | Err(Fail::Client(_)) => {}
            Err(Fail::State(e)) => {
                tracing::warn!(hypothesis = %h.id, error = %e, "cannot remove a hypothesis")
            }
        }
    }
    if let Err(e) = items::each(config, &ended, items::resolve) {
        tracing::warn!(error = %e.message, "cannot resolve expired hypothesis items");
    }
}

#[cfg(test)]
mod tests;
