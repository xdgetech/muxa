//! The Loop check of an assistant run (ADR 0048 §4): muxa's own validation
//! of the result file, and its store on the request when it passes; a plan
//! run's goes to `plan::check` (ADR 0049 §5). It runs
//! in the daemon's process. Reasons go to `.muxa/checks/<attempt>.log` for the
//! agent; host logs get ids and counts only.
use std::path::Path;
use std::time::Instant;

use muxa_protocol::{AssistantRequest, Delivery, WorkItem};

use super::record::{RequestRun, RunFor};
use super::result::{self, Refused, Stored};
use super::store::Shelf;
use crate::loops::step::CheckEnd;
use crate::records::{Loop, LoopState, Reason, Record, now};
use crate::routines::check::report;
use crate::routines::store::StateError;
use crate::task_records::Config;

/// Blocking. Exit 0 once the result is stored (also when it was already).
pub fn run(config: &Config, record: &Record, attempt: u32) -> CheckEnd {
    let started = Instant::now();
    let (outcome, what) = match record.assistant_run() {
        Some(RunFor::Plan(run)) => (super::plan::check::publish(config, record, &run), "plan"),
        _ => (publish(config, record), "assistant result"),
    };
    let seconds = started.elapsed().as_secs();
    let (code, lines) = match &outcome {
        Ok(Stored::New) => (0, vec![format!("passed: the {what} is stored.")]),
        Ok(Stored::Already) => (0, vec![format!("passed: this {what} was stored before.")]),
        Err(reasons) => (1, reasons.clone()),
    };
    match &outcome {
        Ok(stored) => {
            tracing::info!(task = %record.id, attempt, stored = ?stored, what, "assistant result stored")
        }
        Err(r) => {
            tracing::info!(task = %record.id, attempt, reasons = r.len(), what, "assistant result refused")
        }
    }
    if let Some(dir) = &record.dir {
        if let Err(e) = report(dir, attempt, code == 0, &lines, &format!("the {what}")) {
            tracing::warn!(task = %record.id, error = %e, "cannot write the assistant check log");
        }
    }
    CheckEnd::Exited { code, seconds }
}

fn publish(config: &Config, record: &Record) -> Result<Stored, Vec<String>> {
    let one = |r: String| vec![r];
    let run = record
        .request_run()
        .ok_or_else(|| one("This record is not a readable assistant run.".into()))?;
    let dir = record
        .dir
        .as_ref()
        .ok_or_else(|| one("The run has no directory.".into()))?;
    let found = result::read(dir, &run)?;
    let state_dir = config
        .state_dir
        .as_ref()
        .ok_or_else(|| one("This host keeps no records.".into()))?;
    let unknown = unknown_duplicates(state_dir, &found).map_err(|e| {
        tracing::warn!(task = %record.id, error = %e, "cannot read work items");
        one(retry())
    })?;
    if !unknown.is_empty() {
        return Err(unknown
            .iter()
            .map(|i| format!("possibleDuplicates[{i}].work is not a Work item on this host."))
            .collect());
    }
    let requests = Shelf::<AssistantRequest>::open(state_dir).map_err(|e| {
        tracing::warn!(task = %record.id, error = %e, "cannot open assistant requests");
        one(retry())
    })?;
    let at = now();
    let notes = found.notes.clone();
    let stored = requests.update::<_, Failure>(&run.request, |q| {
        result::store(q, found, at).map_err(Failure::Refused)
    });
    match stored {
        Ok(Some(s)) => {
            if let (Stored::New, Some(notes)) = (s, &notes) {
                super::apply::auto_apply(config, record, &run.request, run.version, notes);
            }
            Ok(s)
        }
        Ok(None) => Err(one(format!(
            "Request {} is no longer on this host; the result is not stored.",
            run.request
        ))),
        Err(Failure::Refused(Refused::NotCurrent { current, .. })) => Err(one(format!(
            "This run answers version {} of request {}, which is no longer the request's current \
             version ({current}). The result is not stored.",
            run.version, run.request
        ))),
        Err(Failure::Refused(Refused::Cancelled(_))) => Err(one(
            "The request was cancelled; the result is not stored.".into(),
        )),
        Err(Failure::State(e)) => {
            tracing::warn!(task = %record.id, error = %e, "cannot store an assistant result");
            Err(one(retry()))
        }
    }
}

fn retry() -> String {
    "muxa could not store the result. It checks again after your next turn.".into()
}

/// The indices of `possibleDuplicates` that name no Work item.
fn unknown_duplicates(
    state_dir: &Path,
    found: &muxa_protocol::AssistResult,
) -> Result<Vec<usize>, StateError> {
    if found.possible_duplicates.is_empty() {
        return Ok(Vec::new());
    }
    let works = Shelf::<WorkItem>::open(state_dir)?;
    let mut unknown = Vec::new();
    for (i, d) in found.possible_duplicates.iter().enumerate() {
        if works.get(&d.work)?.is_none() {
            unknown.push(i);
        }
    }
    Ok(unknown)
}

#[derive(Debug, thiserror::Error)]
enum Failure {
    #[error(transparent)]
    Refused(Refused),
    #[error(transparent)]
    State(#[from] StateError),
}

/// Blocking. After the Loop stopped an assistant run without a result, its
/// request version is `failed` (ADR 0048 §4). A stop by the user or by a
/// status change is a cancel or a cleanup, not a failure.
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
    let run = match record.assistant_run() {
        Some(RunFor::Request(run)) => run,
        Some(RunFor::Plan(_)) => return super::plan::check::stopped(state_dir, record, lp),
        None => return,
    };
    match Shelf::open(state_dir).and_then(|s| fail(&s, &run, now())) {
        Ok(true) => {
            tracing::info!(task = %record.id, request = %run.request, version = run.version, "assistant request failed")
        }
        Ok(false) => {}
        Err(e) => {
            tracing::warn!(task = %record.id, request = %run.request, error = %e, "cannot mark an assistant request failed")
        }
    }
}

/// Marks `run`'s version failed, unless the request moved on, has its
/// result, or is being cancelled. True when it changed.
pub fn fail(
    requests: &Shelf<AssistantRequest>,
    run: &RequestRun,
    at: u64,
) -> Result<bool, StateError> {
    let changed = requests.update::<_, StateError>(&run.request, |q| {
        let settled = matches!(
            q.delivery,
            Delivery::Failed | Delivery::CancelRequested | Delivery::Cancelled
        );
        if q.version != run.version || settled || q.results.iter().any(|r| r.version == run.version)
        {
            return Ok(false);
        }
        q.delivery = Delivery::Failed;
        q.delivery_at = at;
        q.updated_at = at;
        Ok(true)
    })?;
    Ok(changed.unwrap_or(false))
}

#[cfg(test)]
#[path = "check_tests.rs"]
mod tests;
