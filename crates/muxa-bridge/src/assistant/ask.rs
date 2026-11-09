//! `request-assistant` (ADR 0046 §3, §4): one request version per Work item
//! `rev`, and one run per version. The run is a record of kind
//! `assistant-run` made by `create()`, found again after a crash by its
//! goal's last line, as `start.rs` finds a Task.
use std::sync::{Arc, Mutex};

use muxa_protocol::{AssistScope, AssistantRequest, Delivery, ErrorBody, ErrorCode, WorkItem};
use serde_json::json;

use super::brief::{Brief, marker};
use super::policy::{self, Delegated, Starting};
use super::record::RunFor;
use super::store::{Extra, Shelf};
use super::work::{Fail, check_rev, closed};
use crate::convert::error_body;
use crate::records::{Record, Status};
use crate::task_records::{Config, Context, NewTask, create};

/// Seconds a `creating` claim holds the version before another may create its run.
pub const CREATING_LIMIT: u64 = 180;
/// The request file's claim, `[version, since]`.
pub const CREATING_FIELD: &str = "creating";
/// The request file's Work item `rev` that made the current version.
const ASKED_FIELD: &str = "askedRev";
/// Bytes, as the brief keeps.
const MAX_PURPOSE: usize = 4096;

/// Budgets are counted and claims taken one at a time. Only the daemon runs
/// requests, so a lock in its process covers every writer.
static CLAIMS: Mutex<()> = Mutex::new(());

#[derive(Debug, Clone)]
pub struct Ask {
    pub work: String,
    pub rev: u64,
    pub scope: AssistScope,
    pub workspace: Option<String>,
    pub purpose: Option<String>,
}

/// What `begin` decided.
enum Step {
    /// The version exists; its run may still need making.
    Known(String, u64),
    /// A new version, claimed for its run.
    Claimed(Box<AssistantRequest>),
}

/// The Work item and request to reply with, and an error when the run did
/// not start (the request is then `failed`).
pub type Asked = (WorkItem, AssistantRequest, Option<ErrorBody>);

pub async fn ask(cx: &Context, a: Ask) -> Result<Asked, ErrorBody> {
    check_purpose(a.purpose.as_deref())?;
    let at = crate::records::now();
    let (config, fallback, work) = (
        Arc::clone(&cx.records),
        cx.tasks.command.clone(),
        a.work.clone(),
    );
    let claimed = super::blocking(move || {
        let delegated = delegate_for(&config, &a, &fallback)?;
        let _one = CLAIMS
            .lock()
            .map_err(|_| error_body(ErrorCode::Internal, "the assistant claim lock broke"))?;
        let (id, q) = match begin(&config, &a, &delegated, at)? {
            Step::Claimed(q) => (q.id.clone(), Some(*q)),
            Step::Known(id, version) => {
                let q = claim_run(&config, &delegated, &id, version, at)?;
                (id, q)
            }
        };
        Ok((id, q.map(|q| (q, delegated))))
    })
    .await?;
    let (request, claimed) = claimed;
    let error = match claimed {
        Some((q, delegated)) => start_run(cx, q, &delegated).await,
        None => None,
    };
    let config = Arc::clone(&cx.records);
    let (item, q) = super::blocking(move || reply(&config, &work, &request)).await?;
    Ok((item, q, error))
}

/// Blocking. The request's workspace and its delegate; the refusals of §7.
fn delegate_for(config: &Config, a: &Ask, fallback: &str) -> Result<Delegated, ErrorBody> {
    let item = super::shelf::<WorkItem>(config)?
        .get(&a.work)
        .map_err(|e| super::state_error(&e))?
        .ok_or_else(|| bad(format!("no Work item {}", a.work)))?;
    let workspace = a
        .workspace
        .clone()
        .or(item.workspace)
        .ok_or_else(|| bad(format!("Work item {} has no workspace; send one", a.work)))?;
    policy::delegated(&config.load_workspaces()?, &workspace, fallback)
}

/// Blocking. The refusals `ask` gives before any write, for `capture {assist}`
/// to check before it sorts the Capture.
pub fn precheck(config: &Config, workspace: &str, fallback: &str) -> Result<(), ErrorBody> {
    let delegated = policy::delegated(&config.load_workspaces()?, workspace, fallback)?;
    let _one = CLAIMS
        .lock()
        .map_err(|_| error_body(ErrorCode::Internal, "the assistant claim lock broke"))?;
    budget(config, &delegated, None, crate::records::now())
}

/// Blocking. Under the Work item's lock: the version this `rev` made, or a
/// new version with its run claimed.
fn begin(config: &Config, a: &Ask, d: &Delegated, at: u64) -> Result<Step, ErrorBody> {
    let works = super::shelf::<WorkItem>(config)?;
    let requests = super::shelf::<AssistantRequest>(config)?;
    let step = works.update(&a.work, |w| -> Result<Step, Fail> {
        let found = find(&requests, &w.id, &d.workspace)?;
        if let Some((q, extra)) = &found {
            if asked(extra) == Some(a.rev) {
                link(w, &q.id, a.rev, at);
                return Ok(Step::Known(q.id.clone(), q.version));
            }
        }
        check_rev("Work item", &w.id, w.rev, a.rev)?;
        if closed(w.state) {
            return Err(bad(format!(
                "Work item {} is done or withdrawn; reopen it first",
                w.id
            ))
            .into());
        }
        let replacing = found.as_ref().map(|(q, _)| q.id.as_str());
        budget(config, d, replacing, at)?;
        let q = match found {
            Some((q, _)) => next_version(&requests, &q.id, a, at)?,
            None => new_request(&requests, w, d, a, at)?,
        };
        if !w.requests.contains(&q.id) {
            w.requests.push(q.id.clone());
        }
        w.rev += 1;
        w.updated_at = at;
        Ok(Step::Claimed(Box::new(q)))
    })?;
    step.ok_or_else(|| bad(format!("no Work item {}", a.work)))
}

/// The Work item's request to `workspace`, with its file's own fields.
fn find(
    requests: &Shelf<AssistantRequest>,
    work: &str,
    workspace: &str,
) -> Result<Option<(AssistantRequest, Extra)>, Fail> {
    let mine = requests
        .all()?
        .into_iter()
        .filter(|q| q.work == work && q.workspace == workspace)
        .min_by_key(|q| (q.created_at, q.id.clone()));
    match mine {
        Some(q) => Ok(requests.get_with_extra(&q.id)?),
        None => Ok(None),
    }
}

fn asked(extra: &Extra) -> Option<u64> {
    extra.get(ASKED_FIELD).and_then(serde_json::Value::as_u64)
}

/// A resend of `rev`: the link and the `rev` bump a crash may have lost.
fn link(w: &mut WorkItem, request: &str, rev: u64, at: u64) {
    let unlinked = !w.requests.iter().any(|r| r == request);
    if unlinked {
        w.requests.push(request.to_owned());
    }
    if unlinked || w.rev == rev {
        w.rev += 1;
        w.updated_at = at;
    }
}

fn claim(version: u64, at: u64, rev: u64) -> Extra {
    let mut extra = Extra::new();
    extra.insert(ASKED_FIELD.into(), rev.into());
    extra.insert(CREATING_FIELD.into(), json!([version, at]));
    extra
}

fn new_request(
    requests: &Shelf<AssistantRequest>,
    w: &WorkItem,
    d: &Delegated,
    a: &Ask,
    at: u64,
) -> Result<AssistantRequest, Fail> {
    requests.create_with_extra(|id| {
        let q = AssistantRequest {
            id,
            work: w.id.clone(),
            version: 1,
            workspace: d.workspace.clone(),
            scope: a.scope,
            delivery: Delivery::Pending,
            delivery_at: at,
            created_at: at,
            updated_at: at,
            purpose: a.purpose.clone(),
            run: None,
            results: Vec::new(),
        };
        Ok::<_, Fail>((q, claim(1, at, a.rev)))
    })
}

/// A newer version makes the older results stale (ADR 0046 §4).
fn next_version(
    requests: &Shelf<AssistantRequest>,
    id: &str,
    a: &Ask,
    at: u64,
) -> Result<AssistantRequest, Fail> {
    let q = requests.update_with_extra(id, |q, extra| -> Result<_, Fail> {
        q.version += 1;
        q.scope = a.scope;
        q.purpose = a.purpose.clone();
        q.run = None;
        q.delivery = Delivery::Pending;
        q.delivery_at = at;
        q.updated_at = at;
        for r in &mut q.results {
            r.stale = true;
        }
        extra.extend(claim(q.version, at, a.rev));
        Ok(q.clone())
    })?;
    q.ok_or_else(|| bad(format!("no request {id}")).into())
}

/// Blocking, under [`CLAIMS`]. Under the request's lock: None when the
/// version has its run (found again by marker after a crash), or needs none.
fn claim_run(
    config: &Config,
    d: &Delegated,
    id: &str,
    version: u64,
    at: u64,
) -> Result<Option<AssistantRequest>, ErrorBody> {
    let requests = super::shelf::<AssistantRequest>(config)?;
    let records = config.records_if_any();
    let claimed = requests.update_with_extra(id, |q, extra| -> Result<_, Fail> {
        if q.version != version || q.run.is_some() || super::delivery::settled(q.delivery) {
            return Ok(None);
        }
        if let Some(r) = existing(&records, &q.id, version) {
            q.run = Some(r.id.clone());
            q.updated_at = at;
            extra.remove(CREATING_FIELD);
            tracing::info!(request = %q.id, version, task = %r.id, "assistant run found again");
            return Ok(None);
        }
        if creating(extra, version).is_some_and(|since| at.saturating_sub(since) < CREATING_LIMIT) {
            return Err(error_body(
                ErrorCode::Conflict,
                format!("a run for request {} is starting; wait for it", q.id),
            )
            .into());
        }
        budget(config, d, Some(&q.id), at)?;
        extra.insert(CREATING_FIELD.into(), json!([version, at]));
        Ok(Some(q.clone()))
    })?;
    Ok(claimed.flatten())
}

/// The claim's start, when it is for `version`.
pub fn creating(extra: &Extra, version: u64) -> Option<u64> {
    let claim = extra.get(CREATING_FIELD)?;
    (claim.get(0)?.as_u64()? == version).then_some(claim.get(1)?.as_u64()?)
}

/// The run that `marker` names. Only a record made for the version counts:
/// a goal's text alone cannot claim another request's run.
pub fn existing<'a>(records: &'a [Record], request: &str, version: u64) -> Option<&'a Record> {
    let mark = marker(request, version);
    records.iter().find(|r| {
        r.assistant_run()
            .is_some_and(|run| run.request == request && run.version == version)
            && r.goal.lines().next_back() == Some(mark.as_str())
    })
}

/// Blocking. Refuses a run past a budget. `replacing` is a request whose
/// current run the new version ends.
fn budget(
    config: &Config,
    d: &Delegated,
    replacing: Option<&str>,
    at: u64,
) -> Result<(), ErrorBody> {
    let requests = super::shelf::<AssistantRequest>(config)?;
    let mut starting = Vec::new();
    for q in requests
        .all()
        .map_err(|e| super::state_error(&e))?
        .into_iter()
        .filter(|q| q.run.is_none())
    {
        let Ok(Some((q, extra))) = requests.get_with_extra(&q.id) else {
            continue;
        };
        if let Some(since) = creating(&extra, q.version) {
            if at.saturating_sub(since) < CREATING_LIMIT {
                starting.push(Starting {
                    request: q.id,
                    workspace: q.workspace,
                    since,
                });
            }
        }
    }
    let (day_start, reset) = policy::day(at);
    let records = config.records_if_any();
    let usage = policy::usage(&records, &starting, &d.workspace, replacing, day_start);
    policy::check(usage, d, &reset)
}

/// Makes the claimed version's run, and ends the claim. The error when it
/// did not start.
async fn start_run(cx: &Context, q: AssistantRequest, d: &Delegated) -> Option<ErrorBody> {
    let config = Arc::clone(&cx.records);
    let (work_id, req) = (q.work.clone(), q.clone());
    let made = super::blocking(move || {
        let item = super::shelf::<WorkItem>(&config)?
            .get(&work_id)
            .map_err(|e| super::state_error(&e))?
            .ok_or_else(|| bad(format!("no Work item {work_id}")))?;
        Ok(Brief::gather(&config, &item, &req)?.goal())
    })
    .await;
    let (record, error) = match made {
        Err(e) => (None, Some(e)),
        Ok(goal) => {
            tracing::info!(request = %q.id, version = q.version, workspace = %q.workspace, "assistant run starting");
            let new = NewTask {
                workspace: q.workspace.clone(),
                worker: d.worker.clone(),
                title: title(&q),
                goal,
                assistant: Some(RunFor {
                    request: q.id.clone(),
                    version: q.version,
                    work: q.work.clone(),
                }),
                ..NewTask::default()
            };
            match cx.env().await {
                Ok(env) => create(&env, new).await,
                Err(e) => (None, Some(e)),
            }
        }
    };
    let config = Arc::clone(&cx.records);
    let task = record.map(|r| r.id);
    let why = error.as_ref().map(|e| e.message.clone());
    let (id, version) = (q.id.clone(), q.version);
    let finished = super::blocking(move || finish(&config, &id, version, task, why)).await;
    if let Err(e) = finished {
        tracing::warn!(request = %q.id, error = %e.message, "cannot record an assistant run");
    }
    error
}

/// Blocking. Ends the claim; links the run, or marks the version `failed`.
fn finish(
    config: &Config,
    id: &str,
    version: u64,
    task: Option<String>,
    why: Option<String>,
) -> Result<(), ErrorBody> {
    if let (Some(task), Some(why)) = (&task, &why) {
        super::tick::drop_run(config, task, &format!("The run did not start: {why}"));
    }
    let at = crate::records::now();
    super::shelf::<AssistantRequest>(config)?.update_with_extra(
        id,
        |q, extra| -> Result<(), Fail> {
            if creating(extra, version).is_some() {
                extra.remove(CREATING_FIELD);
            }
            if q.version != version {
                return Ok(());
            }
            if task.is_some() {
                q.run = task.clone();
            }
            if why.is_some() && !super::delivery::settled(q.delivery) {
                q.delivery = Delivery::Failed;
                q.delivery_at = at;
            }
            q.updated_at = at;
            Ok(())
        },
    )?;
    match (&task, &why) {
        (_, Some(_)) => {
            tracing::warn!(request = id, version, task, "assistant run did not start")
        }
        _ => tracing::info!(request = id, version, task, "assistant run started"),
    }
    Ok(())
}

/// Blocking. The Work item and the request, as a list shows them.
pub fn reply(
    config: &Config,
    work: &str,
    request: &str,
) -> Result<(WorkItem, AssistantRequest), ErrorBody> {
    let item = super::shelf::<WorkItem>(config)?
        .get(work)
        .map_err(|e| super::state_error(&e))?
        .ok_or_else(|| bad(format!("no Work item {work}")))?;
    let q = super::shelf::<AssistantRequest>(config)?
        .get(request)
        .map_err(|e| super::state_error(&e))?
        .ok_or_else(|| bad(format!("no request {request}")))?;
    let mut one = [q];
    super::delivery::refresh(config, &mut one, crate::records::now());
    let [q] = one;
    Ok((super::listed(config, item), q))
}

/// Ids only: the title names the run's folder and session, which logs and
/// tmux show, and a Work item's title comes from Capture text (ADR 0046 §11).
fn title(q: &AssistantRequest) -> String {
    format!(
        "{} {} v{}",
        super::brief::scope_name(q.scope),
        q.id,
        q.version
    )
}

fn check_purpose(purpose: Option<&str>) -> Result<(), ErrorBody> {
    match purpose {
        Some(p) if p.len() > MAX_PURPOSE || p.contains('\0') => Err(bad(format!(
            "purpose is at most {MAX_PURPOSE} bytes, without NUL"
        ))),
        _ => Ok(()),
    }
}

fn bad(message: String) -> ErrorBody {
    error_body(ErrorCode::BadRequest, message)
}

/// The record states a run is open in.
pub fn open(r: &Record) -> bool {
    matches!(r.status, Status::Open | Status::Review)
}

#[cfg(test)]
#[path = "ask_tests.rs"]
mod tests;
