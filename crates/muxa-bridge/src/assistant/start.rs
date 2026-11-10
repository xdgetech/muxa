//! `create-task {work, workRev}`: one Task per Work item `rev`, found again
//! after a crash by the goal's last line (ADR 0048 §3), as Routine approval is.
use std::sync::Arc;

use muxa_protocol::{ErrorBody, ErrorCode, WorkItem};

use super::work::{Fail, check_rev, closed};
use crate::convert::error_body;
use crate::records::{LoopSpec, Record};
use crate::task_records::{Config, Context, NewTask};

/// Seconds a start holds the item before another may start it.
const STARTING_LIMIT: u64 = 180;
/// The Work item's file field that holds a start in flight: `[rev, since]`.
const STARTING_FIELD: &str = "starting";

enum Step {
    /// The Task this `rev` already made.
    Done(Box<Record>),
    Create,
}

/// The Task's record, if one was kept, and the request's error.
pub async fn create_task(
    cx: &Context,
    mut new: NewTask,
    spec: Option<LoopSpec>,
    work: String,
    rev: u64,
) -> Result<(Option<Record>, Option<ErrorBody>), ErrorBody> {
    let at = crate::records::now();
    let (config, id) = (Arc::clone(&cx.records), work.clone());
    let step = super::blocking(move || begin(&config, &id, rev, at)).await?;
    if let Step::Done(record) = step {
        return Ok((Some(*record), None));
    }
    new.goal = format!("{}\n\n{}", new.goal.trim_end(), marker(&work, rev));
    new.work = Some(work.clone());
    let made = crate::task_records::create_with_loop(cx, new, spec).await;
    let task = made
        .as_ref()
        .ok()
        .and_then(|(r, _)| r.as_ref())
        .map(|r| r.id.clone());
    let config = Arc::clone(&cx.records);
    let linked = super::blocking(move || finish(&config, &work, task.as_deref())).await;
    if let Err(e) = linked {
        // The Task exists; a resend links it.
        tracing::warn!(error = %e.message, "cannot link a task to its work item");
    }
    made
}

/// `muxa-work: <work> <rev>`: the goal's last line.
fn marker(work: &str, rev: u64) -> String {
    format!("muxa-work: {work} {rev}")
}

/// The Task `marker` names. Only a record started for the item counts:
/// a goal's text alone cannot claim another item's Task.
fn existing(config: &Config, work: &str, rev: u64) -> Option<Record> {
    let mark = marker(work, rev);
    config
        .records_if_any()
        .into_iter()
        .find(|r| r.work() == Some(work) && r.goal.lines().next_back() == Some(mark.as_str()))
}

/// Blocking. Under the item's lock: the Task this `rev` made, or a claim to make it.
fn begin(config: &Config, work: &str, rev: u64, at: u64) -> Result<Step, ErrorBody> {
    let works = super::shelf::<WorkItem>(config)?;
    let step = works.update_with_extra(work, |w, extra| -> Result<Step, Fail> {
        if let Some(record) = existing(config, work, rev) {
            if !w.tasks.contains(&record.id) {
                w.tasks.push(record.id.clone());
                w.rev += 1;
                w.updated_at = at;
            }
            extra.remove(STARTING_FIELD);
            return Ok(Step::Done(Box::new(record)));
        }
        check_rev("Work item", &w.id, w.rev, rev)?;
        if closed(w.state) {
            return Err(error_body(
                ErrorCode::BadRequest,
                format!("Work item {} is done or withdrawn; reopen it first", w.id),
            )
            .into());
        }
        let since = extra
            .get(STARTING_FIELD)
            .and_then(|v| v.get(1))
            .and_then(serde_json::Value::as_u64);
        if since.is_some_and(|s| at.saturating_sub(s) < STARTING_LIMIT) {
            return Err(error_body(
                ErrorCode::Conflict,
                format!("a Task for Work item {} is starting; wait for it", w.id),
            )
            .into());
        }
        extra.insert(STARTING_FIELD.into(), serde_json::json!([rev, at]));
        Ok(Step::Create)
    })?;
    step.ok_or_else(|| error_body(ErrorCode::BadRequest, format!("no Work item {work}")))
}

/// Blocking. Lists `task` on Work item `work`, as a Task made for it: the
/// writer's Task of an approved write (ADR 0052 §3).
pub fn link_task(config: &Config, work: &str, task: &str) {
    if let Err(e) = finish(config, work, Some(task)) {
        tracing::warn!(work, task, error = %e.message, "cannot list a task on its work item");
    }
}

/// Blocking. Ends the claim and lists the Task, if one was made.
fn finish(config: &Config, work: &str, task: Option<&str>) -> Result<(), ErrorBody> {
    let works = super::shelf::<WorkItem>(config)?;
    let at = crate::records::now();
    works.update_with_extra(work, |w, extra| -> Result<(), Fail> {
        extra.remove(STARTING_FIELD);
        if let Some(task) = task.filter(|t| !w.tasks.iter().any(|x| x == t)) {
            w.tasks.push(task.to_owned());
            w.rev += 1;
            w.updated_at = at;
        }
        Ok(())
    })?;
    if let Some(task) = task {
        tracing::info!(work, task, "task started for a work item");
    }
    Ok(())
}
