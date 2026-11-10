//! A request version's chain of steps (ADR 0051 §3): one run each, found
//! again after a crash by the goal line `muxa-request: q-… v<n> s<k>`, and
//! claimed while being made. A request without `steps` is one step.
use muxa_protocol::{AssistantRequest, Delivery, RequestStep, StepBy};
use serde_json::json;

use super::store::Extra;
use crate::records::Record;

/// The request file's claim, `[version, since, step]`; a claim without
/// `step` is step 1's, as a host before ADR 0051 wrote it.
pub const CREATING_FIELD: &str = "creating";

/// The step the request's `run` and `delivery` are about: its newest.
pub fn current(q: &AssistantRequest) -> u32 {
    q.steps.last().map_or(1, |s| s.step)
}

/// A new version's first step, when the workspace has a staff.
pub fn first(worker: Option<&str>, by: StepBy, at: u64) -> Vec<RequestStep> {
    worker
        .map(|w| vec![fresh(1, w, by, at)])
        .unwrap_or_default()
}

/// A step not started yet.
pub fn fresh(step: u32, worker: &str, by: StepBy, at: u64) -> RequestStep {
    RequestStep {
        step,
        worker: worker.to_owned(),
        by,
        delivery: Delivery::Pending,
        delivery_at: at,
        run: None,
        handoff: None,
    }
}

/// Copies the request's `delivery` and `run` to its newest step, which they describe.
pub fn sync(q: &mut AssistantRequest) {
    let (delivery, at, run) = (q.delivery, q.delivery_at, q.run.clone());
    if let Some(last) = q.steps.last_mut() {
        last.delivery = delivery;
        last.delivery_at = at;
        if run.is_some() {
            last.run = run;
        }
    }
}

/// `muxa-request: <request> v<version>`, with ` s<step>` after the first
/// step: the goal's last line, which finds a run again after a crash.
pub fn marker(request: &str, version: u64, step: u32) -> String {
    match step {
        0 | 1 => format!("muxa-request: {request} v{version}"),
        k => format!("muxa-request: {request} v{version} s{k}"),
    }
}

pub fn claim(version: u64, step: u32, at: u64) -> serde_json::Value {
    json!([version, at, step])
}

/// The claim's start, when it is for `version` and `step`.
pub fn creating(extra: &Extra, version: u64, step: u32) -> Option<u64> {
    let claim = extra.get(CREATING_FIELD)?;
    let claimed = claim
        .get(2)
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(1);
    (claim.get(0)?.as_u64()? == version && claimed == u64::from(step))
        .then_some(claim.get(1)?.as_u64()?)
}

/// The run made for `step` of `version`. Only a record made for it counts:
/// a goal's text alone cannot claim another request's run.
pub fn existing<'a>(
    records: &'a [Record],
    request: &str,
    version: u64,
    step: u32,
) -> Option<&'a Record> {
    let mark = marker(request, version, step);
    records.iter().find(|r| {
        r.request_run().is_some_and(|run| {
            run.request == request && run.version == version && run.step() == step
        }) && r.goal.lines().next_back() == Some(mark.as_str())
    })
}

#[cfg(test)]
#[path = "steps_tests.rs"]
mod tests;
