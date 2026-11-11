//! `update-inbox retry` on an assistant run's `failure` item (ADR 0061): a
//! failed request is asked again as a new version, a failed plan runs again
//! through `plan-now`, and the secretary's give-up resets its restart cap.
//! The item's stored key names what failed. One `Approval` keyed by that
//! key makes a second retry return what the first made.
use std::sync::Arc;

use muxa_protocol::{
    AssistantRequest, Delivery, ErrorBody, ErrorCode, InboxAction, InboxItem, ItemKind, ItemState,
    Message, Plan, StepBy, WorkItem,
};

use super::approval::{self, Approval};
use super::approve::{Claim, Found, bad, claim, release, stored, with_items};
use crate::assistant::ask::{self, Ask};
use crate::assistant::plan::start::{self, How};
use crate::assistant::{NO_DAEMON, blocking, shelf, state_error};
use crate::convert::error_body;
use crate::items::store::ItemStore;
use crate::items::{answers, shown};
use crate::records::now;
use crate::task_records::{Config, Context};

/// What a failure item's key says failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Failed {
    Request { request: String, version: u64 },
    Plan(String),
    Secretary,
}

/// Pure. The keys `failures.rs` and the secretary's give-up write.
pub fn failed(key: &str) -> Option<Failed> {
    let rest = key.strip_prefix("failure:")?;
    if rest.starts_with("secretary:") {
        return Some(Failed::Secretary);
    }
    if rest.starts_with("p-") && !rest.contains(':') {
        return Some(Failed::Plan(rest.to_owned()));
    }
    let mut parts = rest.split(':');
    let (request, version) = (parts.next()?, parts.next()?.parse().ok()?);
    parts.next()?.parse::<u32>().ok()?;
    (request.starts_with("q-") && parts.next().is_none()).then(|| Failed::Request {
        request: request.to_owned(),
        version,
    })
}

/// `update-inbox retry` on an assistant failure item: `work-saved` or
/// `plan-saved` with what it started, then `inbox-saved`. None for any
/// other item, which `items::answer` handles.
pub async fn answer(
    cx: &Context,
    id: u64,
    entry: &str,
) -> Option<(Vec<Message>, Option<ErrorBody>)> {
    let (config, item) = (Arc::clone(&cx.records), entry.to_owned());
    let (found, what) = match blocking(move || find(&config, &item)).await {
        Ok(Some(found)) => found,
        Ok(None) => return None,
        Err(e) => return Some((Vec::new(), Some(e))),
    };
    let done = retry(cx, found, what).await;
    Some(match done {
        Ok((made, item, error)) => {
            let mut out: Vec<Message> = made.into_iter().map(|m| m.message(id)).collect();
            out.push(Message::InboxSaved {
                inbox: id,
                entry: None,
                item: Some(Box::new(item)),
            });
            (out, error)
        }
        Err(e) => (Vec::new(), Some(e)),
    })
}

/// What a retry started, for its reply.
enum Made {
    Asked(WorkItem, AssistantRequest),
    Plan(Plan),
}

impl Made {
    fn message(self, id: u64) -> Message {
        match self {
            Made::Asked(entry, request) => Message::WorkSaved {
                work: id,
                entry,
                request: Some(Box::new(request)),
            },
            Made::Plan(entry) => Message::PlanSaved {
                plan: id,
                entry: Box::new(entry),
            },
        }
    }
}

type Retried = (Option<Made>, InboxItem, Option<ErrorBody>);

/// Blocking. An assistant run's failure item, with its approval keyed by
/// the item's key; None for any other item.
fn find(config: &Config, id: &str) -> Result<Option<(Found, Failed)>, ErrorBody> {
    let Some(dir) = config.state_dir.as_ref().filter(|d| d.is_dir()) else {
        return Ok(None);
    };
    let store = ItemStore::open(dir).map_err(|e| state_error(&e))?;
    let Some(task) = store.owner(id).map_err(|e| state_error(&e))? else {
        return Ok(None);
    };
    let items = store.get(&task).map_err(|e| state_error(&e))?;
    let Some(s) = items.items.iter().find(|s| s.item.id == id) else {
        return Ok(None);
    };
    if s.item.kind != ItemKind::Failure || s.item.task.is_some() {
        return Ok(None);
    }
    let Some(what) = failed(&s.key) else {
        return Ok(None);
    };
    let hash = format!("retry:{}", s.key);
    Ok(Some((
        Found {
            approval: approval::get(&items, id, &hash),
            item: s.item.clone(),
            task,
            hash,
        },
        what,
    )))
}

async fn retry(cx: &Context, f: Found, what: Failed) -> Result<Retried, ErrorBody> {
    if f.approval.as_ref().is_some_and(Approval::done) {
        let config = Arc::clone(&cx.records);
        return blocking(move || again(&config, &f)).await;
    }
    if !matches!(f.item.state, ItemState::Open | ItemState::Later) {
        return Err(bad(format!("Inbox item {} is already closed", f.item.id)));
    }
    if !cx.daemon {
        return Err(error_body(ErrorCode::Unsupported, NO_DAEMON));
    }
    match what {
        Failed::Request { request, version } => ask_again(cx, f, request, version).await,
        Failed::Plan(plan) => plan_again(cx, f, plan).await,
        Failed::Secretary => {
            let config = Arc::clone(&cx.records);
            blocking(move || restart_secretary(&config, &f, now())).await
        }
    }
}

/// What the first retry made, read again.
fn again(config: &Config, f: &Found) -> Result<Retried, ErrorBody> {
    let a = f.approval.clone().unwrap_or_default();
    let made = match (a.work, a.request, a.plan) {
        (Some(work), Some(request), _) => ask::reply(config, &work, &request)
            .ok()
            .map(|(w, q)| Made::Asked(w, q)),
        (_, _, Some(plan)) => crate::assistant::plan::shelf(config)?
            .get(&plan)
            .ok()
            .flatten()
            .map(|p| Made::Plan(crate::assistant::plan::listed(config, p))),
        _ => None,
    };
    Ok((made, shown(&f.item), None))
}

/// The request asked again as it was: its scope, purpose and workspace,
/// and the member its first step named.
async fn ask_again(
    cx: &Context,
    f: Found,
    request: String,
    version: u64,
) -> Result<Retried, ErrorBody> {
    let (config, found, at) = (Arc::clone(&cx.records), f.clone(), now());
    let prepared = blocking(move || prepare(&config, &found, &request, version, at)).await?;
    let asked = match prepared {
        Prepared::Ask(asked) => asked,
        Prepared::Done(done) => {
            let config = Arc::clone(&cx.records);
            return blocking(move || again(&config, &done)).await;
        }
        Prepared::Moved(why) => {
            let config = Arc::clone(&cx.records);
            return blocking(move || resolve(&config, &f, &why)).await;
        }
    };
    let config = Arc::clone(&cx.records);
    match ask::ask(cx, asked).await {
        Ok((w, q, error)) => {
            let (work, req) = (w.id.clone(), q.id.clone());
            let saved = f.clone();
            let item = blocking(move || {
                finish(&config, &saved, now(), |a| {
                    (a.work, a.request) = (Some(work), Some(req));
                })
            })
            .await?;
            tracing::info!(item = %f.item.id, request = %q.id, version = q.version, "failed request asked again");
            Ok((Some(Made::Asked(w, q)), item, error))
        }
        Err(e) => {
            unclaim(cx, f).await;
            Err(e)
        }
    }
}

enum Prepared {
    Ask(Ask),
    /// Another retry finished first.
    Done(Box<Found>),
    /// The request moved on without this retry: the reason the item closes.
    Moved(String),
}

/// Blocking. The claim at the Work item's `rev`, and the request to make.
fn prepare(
    config: &Config,
    f: &Found,
    request: &str,
    version: u64,
    at: u64,
) -> Result<Prepared, ErrorBody> {
    let q = shelf::<AssistantRequest>(config)?
        .get(request)
        .map_err(|e| state_error(&e))?
        .ok_or_else(|| bad(format!("no request {request}")))?;
    let current = shelf::<WorkItem>(config)?
        .get(&q.work)
        .map_err(|e| state_error(&e))?
        .ok_or_else(|| bad(format!("no Work item {}", q.work)))?
        .rev;
    // A resend of the rev an earlier attempt asked at finds its version.
    let stored = f.approval.as_ref().and_then(|a| a.rev);
    let earlier = ask::asked_rev(config, &q.work, &q.workspace)?.filter(|r| Some(*r) == stored);
    if earlier.is_none() && (q.version != version || q.delivery != Delivery::Failed) {
        return Ok(Prepared::Moved(format!(
            "Request {} was asked again as version {}; nothing to retry.",
            q.id, q.version
        )));
    }
    let rev = earlier.unwrap_or(current);
    if let Claim::Done(done) = claim(config, f, rev, super::approve::By::User, at)? {
        return Ok(Prepared::Done(done));
    }
    let first = q.steps.iter().find(|s| s.step == 1);
    let worker = first
        .filter(|s| s.by != StepBy::Lead)
        .map(|s| s.worker.clone());
    Ok(Prepared::Ask(Ask {
        work: q.work.clone(),
        rev,
        scope: q.scope,
        workspace: Some(q.workspace.clone()),
        purpose: q.purpose.clone(),
        worker,
        asker: StepBy::User,
        follows: None,
    }))
}

/// A new plan now, with the failed plan's time; refused past a budget.
async fn plan_again(cx: &Context, f: Found, plan: String) -> Result<Retried, ErrorBody> {
    let (config, found, at) = (Arc::clone(&cx.records), f.clone(), now());
    let checked = blocking(move || plan_claim(&config, &found, &plan, at)).await?;
    let minutes = match checked {
        PlanClaim::Go(minutes) => minutes,
        PlanClaim::Done(done) => {
            let config = Arc::clone(&cx.records);
            return blocking(move || again(&config, &done)).await;
        }
        PlanClaim::Moved(why) => {
            let config = Arc::clone(&cx.records);
            return blocking(move || resolve(&config, &f, &why)).await;
        }
    };
    let config = Arc::clone(&cx.records);
    match start::start(cx, How::Now { at, minutes }).await {
        Ok((p, error)) => {
            let saved = (f.clone(), p.id.clone());
            let (item, entry) = blocking(move || {
                let (f, id) = saved;
                let item = finish(&config, &f, now(), |a| a.plan = Some(id))?;
                Ok((item, listed_plan(&config, p)))
            })
            .await?;
            tracing::info!(item = %f.item.id, plan = %entry.id, "failed plan run again");
            Ok((Some(Made::Plan(entry)), item, error))
        }
        Err(e) => {
            unclaim(cx, f).await;
            Err(e)
        }
    }
}

fn listed_plan(config: &Config, p: Plan) -> Plan {
    crate::assistant::plan::listed(config, p)
}

enum PlanClaim {
    Go(Option<u32>),
    Done(Box<Found>),
    Moved(String),
}

/// Blocking. Claims the retry unless a newer plan replaced the failed one.
fn plan_claim(config: &Config, f: &Found, plan: &str, at: u64) -> Result<PlanClaim, ErrorBody> {
    let mut plans = crate::assistant::plan::shelf(config)?
        .all()
        .map_err(|e| state_error(&e))?;
    crate::assistant::plan::newest_first(&mut plans);
    let failed = plans
        .iter()
        .find(|p| p.id == plan)
        .ok_or_else(|| bad(format!("no plan {plan}")))?;
    let minutes = failed.minutes;
    if let Some(newest) = plans.first().filter(|p| p.id != plan) {
        return Ok(PlanClaim::Moved(format!(
            "A newer plan, {}, was made since; nothing to retry.",
            newest.id
        )));
    }
    match claim(config, f, 0, super::approve::By::User, at)? {
        Claim::Done(done) => Ok(PlanClaim::Done(done)),
        Claim::Go => Ok(PlanClaim::Go(minutes)),
    }
}

/// Blocking. Asks the daemon to start the secretary again with a fresh
/// restart cap (ADR 0056 §2): only its tick starts it, so a retry never
/// opens a second session.
fn restart_secretary(config: &Config, f: &Found, at: u64) -> Result<Retried, ErrorBody> {
    let workspaces = config.load_workspaces()?;
    if workspaces.secretary().is_none() {
        let why = "The assistant's chat is no longer configured to stay open; nothing to retry.";
        return resolve(config, f, why);
    }
    if let Claim::Done(done) = claim(config, f, 0, super::approve::By::User, at)? {
        return again(config, &done);
    }
    let dir = config
        .state_dir
        .as_ref()
        .ok_or_else(|| error_body(ErrorCode::Internal, "the state directory went away"))?;
    if let Err(e) = crate::secretary::file::update(dir, |s| s.retry_at = Some(at)) {
        release(config, f, None, now());
        return Err(state_error(&e));
    }
    let item = finish(config, f, at, |a| a.restarted_at = Some(at))?;
    tracing::info!(item = %f.item.id, "secretary restart asked");
    Ok((None, item, None))
}

/// Blocking. Records what the retry made and answers the item `retry`.
fn finish(
    config: &Config,
    f: &Found,
    at: u64,
    made: impl FnOnce(&mut Approval),
) -> Result<InboxItem, ErrorBody> {
    with_items(config, &f.task, |items| {
        let mut a =
            approval::get(items, &f.item.id, &f.hash).unwrap_or_else(|| Approval::new(&f.hash));
        a.since = None;
        made(&mut a);
        approval::put(items, &f.item.id, a);
        let item = stored(items, &f.item.id)?;
        item.state = ItemState::Answered;
        item.answer = Some(InboxAction::Retry);
        item.answered_at = Some(at);
        item.until = None;
        item.answers = answers(item);
        Ok(shown(item))
    })
}

/// Blocking. Closes an item whose failure was handled another way.
fn resolve(config: &Config, f: &Found, why: &str) -> Result<Retried, ErrorBody> {
    let item = with_items(config, &f.task, |items| {
        let item = stored(items, &f.item.id)?;
        item.state = ItemState::Resolved;
        item.summary = Some(why.to_owned());
        item.until = None;
        item.answers = answers(item);
        Ok(shown(item))
    })?;
    Ok((None, item, None))
}

async fn unclaim(cx: &Context, f: Found) {
    let config = Arc::clone(&cx.records);
    let released = blocking(move || {
        release(&config, &f, None, now());
        Ok(())
    })
    .await;
    if let Err(e) = released {
        tracing::warn!(error = %e.message, "cannot release a retry's claim");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_key_names_what_failed() {
        assert_eq!(
            failed("failure:q-0000000a:3:2"),
            Some(Failed::Request {
                request: "q-0000000a".into(),
                version: 3
            })
        );
        assert_eq!(
            failed("failure:p-0000000b"),
            Some(Failed::Plan("p-0000000b".into()))
        );
        assert_eq!(
            failed("failure:secretary:1762160400"),
            Some(Failed::Secretary)
        );
        for other in [
            "review:1762160400",
            "failure:q-0000000a:x:1",
            "failure:q-0000000a:1",
            "failure:q-0000000a:1:1:1",
            "failure:t-000001",
            "failure:",
        ] {
            assert_eq!(failed(other), None, "{other}");
        }
    }
}
