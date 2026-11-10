//! A step's hand-off in its result file (ADR 0051 §3): "pass this to that
//! member next". The check refuses a worker not on the staff, the run's own,
//! one already in the chain, and any hand-off from the last step or where
//! hand-offs are off. Reasons name fields, never the agent's values.
use muxa_protocol::{AssistantRequest, Handoff};
use serde::Deserialize;

use super::record::RequestRun;
use super::result::text;
use crate::records::Workspaces;
use crate::records::staff::Handoffs;

/// Bytes of a hand-off's purpose.
pub const MAX_PURPOSE: usize = 1024;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RawHandoff {
    worker: String,
    purpose: String,
}

/// What a step's hand-off is checked against.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Chain {
    /// The workspace's staff; empty without `staff`.
    pub staff: Vec<String>,
    /// The workers of steps 1 to `step`, this run's last.
    pub workers: Vec<String>,
    pub step: u32,
    pub max_steps: u32,
    pub handoffs: Handoffs,
}

impl Chain {
    /// The chain `run` belongs to, from the request as stored and the
    /// workspace's configuration now. Without `staff` no hand-off passes.
    pub fn of(workspaces: &Workspaces, q: &AssistantRequest, run: &RequestRun) -> Self {
        let Some(d) = workspaces
            .get(&q.workspace)
            .and_then(|w| w.assistant.as_ref())
        else {
            return Self::default();
        };
        let step = run.step();
        Self {
            staff: d.staff.members.clone(),
            workers: q
                .steps
                .iter()
                .filter(|s| s.step <= step)
                .map(|s| s.worker.clone())
                .collect(),
            step,
            max_steps: d.staff.max_steps,
            handoffs: d.staff.handoffs,
        }
    }
}

/// Keeps a stored result's hand-off on its step; the next step waits for
/// approval, or for the host under `handoffs = "auto"` (ADR 0051 §3).
pub fn keep(q: &mut AssistantRequest, step: u32, handoff: Option<Handoff>) {
    if let Some(s) = q.steps.iter_mut().find(|s| s.step == step) {
        s.handoff = handoff;
    }
}

/// The hand-off, or None with the reasons it fails.
pub(super) fn check(raw: RawHandoff, chain: &Chain, reasons: &mut Vec<String>) -> Option<Handoff> {
    let refuse = |reasons: &mut Vec<String>, why: String| {
        reasons.push(why);
        None
    };
    if chain.staff.is_empty() {
        return refuse(
            reasons,
            "handoff: this workspace has no staff to hand to; leave handoff out.".into(),
        );
    }
    if chain.handoffs == Handoffs::None {
        return refuse(
            reasons,
            "handoff: hand-offs are off in this workspace (handoffs = \"none\"); answer the \
             request yourself and leave handoff out."
                .into(),
        );
    }
    if chain.step >= chain.max_steps {
        return refuse(
            reasons,
            format!(
                "handoff: this is the last step ({} of at most {}); answer the request yourself \
                 and leave handoff out.",
                chain.step, chain.max_steps
            ),
        );
    }
    let before = reasons.len();
    let own = chain.workers.last().map(String::as_str);
    if !chain.staff.contains(&raw.worker) {
        reasons.push(format!(
            "handoff.worker must be a member of the staff: {}.",
            chain.staff.join(", ")
        ));
    } else if own == Some(raw.worker.as_str()) {
        reasons.push("handoff.worker is this run's own worker; name another member.".into());
    } else if chain.workers.contains(&raw.worker) {
        reasons.push(
            "handoff.worker already took a step of this request; a member works once per \
             request."
                .into(),
        );
    }
    text("handoff.purpose", &raw.purpose, MAX_PURPOSE, reasons);
    (reasons.len() == before).then_some(Handoff {
        worker: raw.worker,
        purpose: raw.purpose,
    })
}

#[cfg(test)]
#[path = "handoff_tests.rs"]
mod tests;
