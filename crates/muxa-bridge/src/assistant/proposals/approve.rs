//! Approving a `sort` or `delegate` item (ADR 0049 §4). The stored action
//! runs once, through `sort-capture`'s or `request-assistant`'s own code, at
//! the `rev` read when it is approved. A second approve returns what the
//! first made. The host approves a delegate item itself (`By::Host`) for a
//! workspace with `overall_requests = "auto"`.
use std::sync::Arc;

use muxa_protocol::{
    AssistantRequest, Capture, CaptureState, ErrorBody, ErrorCode, InboxAction, InboxItem,
    ItemKind, ItemState, OverallRequests, WorkItem,
};

use super::approval::{self, Approval};
use crate::assistant::ask::{self, Ask};
use crate::assistant::work::{self, Fail, Sort};
use crate::assistant::{NO_DAEMON, blocking, listed, shelf, state_error};
use crate::convert::error_body;
use crate::items::store::{ItemStore, TaskItems};
use crate::items::{answers, shown};
use crate::records::now;
use crate::task_records::{Config, Context};

/// Seconds an approval's claim holds before another approve may run it again.
pub const APPROVAL_LIMIT: u64 = 180;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum By {
    User,
    /// `overall_requests = "auto"`: the host approves without review.
    Host,
}

/// A proposal item as stored.
#[derive(Debug, Clone)]
pub struct Found {
    /// The plan run whose item file holds it.
    pub task: String,
    pub item: InboxItem,
    pub hash: String,
    pub approval: Option<Approval>,
}

/// What an approval made, for `capture-saved` or `work-saved`.
#[derive(Debug, Clone)]
pub enum Made {
    Sorted(Capture, Option<WorkItem>),
    Asked(WorkItem, AssistantRequest),
}

#[derive(Debug, Clone)]
pub struct Approved {
    pub made: Option<Made>,
    pub item: InboxItem,
    /// The run did not start, or the Capture was sorted already.
    pub error: Option<ErrorBody>,
}

/// The ids an approval records.
#[derive(Debug, Default)]
struct Ids {
    capture: Option<String>,
    work: Option<String>,
    request: Option<String>,
}

enum Claim {
    Go,
    /// Another approve finished first.
    Done(Box<Found>),
}

/// Blocking. Proposal item `id`, or None for any other item.
pub fn find(config: &Config, id: &str) -> Result<Option<Found>, ErrorBody> {
    let Some(dir) = config.state_dir.as_ref().filter(|d| d.is_dir()) else {
        return Ok(None);
    };
    let store = ItemStore::open(dir).map_err(|e| state_error(&e))?;
    let Some(task) = store.owner(id).map_err(|e| state_error(&e))? else {
        return Ok(None);
    };
    let items = store.get(&task).map_err(|e| state_error(&e))?;
    let Some(stored) = items.items.iter().find(|s| s.item.id == id) else {
        return Ok(None);
    };
    let Some(hash) = approval::hash(&stored.item) else {
        return Ok(None);
    };
    Ok(Some(Found {
        approval: approval::get(&items, id, &hash),
        item: stored.item.clone(),
        task,
        hash,
    }))
}

pub async fn approve(cx: &Context, found: Found, by: By) -> Result<Approved, ErrorBody> {
    let config = Arc::clone(&cx.records);
    if found.approval.as_ref().is_some_and(Approval::done) {
        return blocking(move || again(&config, &found)).await;
    }
    if !matches!(found.item.state, ItemState::Open | ItemState::Later) {
        return Err(bad(format!(
            "Inbox item {} is already closed",
            found.item.id
        )));
    }
    match found.item.kind {
        ItemKind::Sort => blocking(move || sort(&config, found, by, now())).await,
        _ => delegate(cx, found, by).await,
    }
}

/// Blocking. What the first approval made, read again.
fn again(config: &Config, f: &Found) -> Result<Approved, ErrorBody> {
    let a = f.approval.clone().unwrap_or_default();
    let work_item = |id: &str| {
        shelf::<WorkItem>(config)
            .ok()
            .and_then(|s| s.get(id).ok().flatten())
            .map(|w| listed(config, w))
    };
    let made = match f.item.kind {
        ItemKind::Sort => a
            .capture
            .as_deref()
            .and_then(|c| shelf::<Capture>(config).ok()?.get(c).ok().flatten())
            .map(|c| Made::Sorted(c, a.work.as_deref().and_then(work_item))),
        _ => match (&a.work, &a.request) {
            (Some(w), Some(q)) => ask::reply(config, w, q)
                .ok()
                .map(|(w, q)| Made::Asked(w, q)),
            _ => None,
        },
    };
    Ok(Approved {
        made,
        item: shown(&f.item),
        error: None,
    })
}

/// Blocking: `sort-capture` at the Capture's current `rev`.
fn sort(config: &Config, f: Found, by: By, at: u64) -> Result<Approved, ErrorBody> {
    let p = f
        .item
        .sort
        .clone()
        .ok_or_else(|| bad("the item has no sort"))?;
    let found = shelf::<Capture>(config)?
        .get(&p.capture)
        .map_err(|e| state_error(&e))?;
    let c = match found {
        Some(c) if c.state == CaptureState::Received => c,
        Some(c) if recovered(f.approval.as_ref(), &c) => {
            let ids = Ids {
                capture: Some(c.id.clone()),
                work: c.work.last().cloned(),
                request: None,
            };
            let work = ids.work.as_deref().and_then(|w| {
                shelf::<WorkItem>(config)
                    .ok()?
                    .get(w)
                    .ok()
                    .flatten()
                    .map(|w| listed(config, w))
            });
            let item = finish(config, &f, ids, by, at)?;
            return Ok(Approved {
                made: Some(Made::Sorted(c, work)),
                item,
                error: None,
            });
        }
        other => {
            let why = match other {
                Some(_) => format!(
                    "Capture {} was sorted before this proposal was approved; nothing changed.",
                    p.capture
                ),
                None => format!(
                    "Capture {} is no longer on this host; nothing changed.",
                    p.capture
                ),
            };
            let item = resolve(config, &f, &why)?;
            tracing::info!(item = %f.item.id, capture = %p.capture, "sort proposal closed: the capture is sorted");
            return Ok(Approved {
                made: None,
                item,
                error: Some(error_body(ErrorCode::Conflict, why)),
            });
        }
    };
    if let Claim::Done(done) = claim(config, &f, c.rev, by, at)? {
        return again(config, &done);
    }
    let s = Sort {
        capture: c.id.clone(),
        rev: c.rev,
        action: p.action,
        work: p.work.clone(),
        title: p.title.clone(),
        workspace: p.workspace.clone(),
    };
    match work::sort(config, s) {
        Ok((c, w)) => {
            let ids = Ids {
                capture: Some(c.id.clone()),
                work: w.as_ref().map(|w| w.id.clone()),
                request: None,
            };
            let item = finish(config, &f, ids, by, at)?;
            tracing::info!(item = %f.item.id, capture = %c.id, by = ?by, "sort proposal approved");
            Ok(Approved {
                made: Some(Made::Sorted(c, w.map(|w| listed(config, w)))),
                item,
                error: None,
            })
        }
        Err(e) => {
            release(config, &f, None, at);
            Err(e)
        }
    }
}

/// The last attempt sorted the Capture and stopped before it was recorded:
/// a sort is one write that adds one to `rev`.
fn recovered(a: Option<&Approval>, c: &Capture) -> bool {
    a.and_then(|a| a.rev).is_some_and(|r| c.rev == r + 1)
}

/// `request-assistant` with the Work item's current `rev` and the receiving workspace.
async fn delegate(cx: &Context, f: Found, by: By) -> Result<Approved, ErrorBody> {
    let p = f
        .item
        .delegate
        .clone()
        .ok_or_else(|| bad("the item has no delegate"))?;
    if !cx.daemon {
        return Err(error_body(ErrorCode::Unsupported, NO_DAEMON));
    }
    let (config, found, at) = (Arc::clone(&cx.records), f.clone(), now());
    let (ws, work) = (p.workspace.clone(), p.work.clone());
    let (claimed, rev) = blocking(move || {
        refuse_none(&config, &ws)?;
        let current = shelf::<WorkItem>(&config)?
            .get(&work)
            .map_err(|e| state_error(&e))?
            .ok_or_else(|| bad(format!("no Work item {work}")))?
            .rev;
        // A resend of the rev an earlier attempt asked at finds its request.
        let stored = found.approval.as_ref().and_then(|a| a.rev);
        let rev = ask::asked_rev(&config, &work, &ws)?
            .filter(|r| Some(*r) == stored)
            .unwrap_or(current);
        Ok((claim(&config, &found, rev, by, at)?, rev))
    })
    .await?;
    if let Claim::Done(done) = claimed {
        let config = Arc::clone(&cx.records);
        return blocking(move || again(&config, &done)).await;
    }
    let a = Ask {
        work: p.work.clone(),
        rev,
        scope: p.scope,
        workspace: Some(p.workspace.clone()),
        purpose: Some(p.purpose.clone()),
    };
    let config = Arc::clone(&cx.records);
    match ask::ask(cx, a).await {
        Ok((w, q, error)) => {
            let ids = Ids {
                capture: None,
                work: Some(w.id.clone()),
                request: Some(q.id.clone()),
            };
            let saved = f.clone();
            let item = blocking(move || finish(&config, &saved, ids, by, now())).await?;
            tracing::info!(item = %f.item.id, request = %q.id, version = q.version, workspace = %p.workspace, by = ?by, "delegate proposal approved");
            Ok(Approved {
                made: Some(Made::Asked(w, q)),
                item,
                error,
            })
        }
        Err(e) => {
            let refusal = (by == By::Host).then(|| e.message.clone());
            let released = blocking(move || {
                release(&config, &f, refusal.as_deref(), now());
                Ok(())
            })
            .await;
            if let Err(r) = released {
                tracing::warn!(error = %r.message, "cannot release a proposal's approval");
            }
            Err(e)
        }
    }
}

/// The overall assistant may not ask a workspace whose `overall_requests` is
/// `none`, or that takes no requests at all.
fn refuse_none(config: &Config, workspace: &str) -> Result<(), ErrorBody> {
    if config.load_workspaces()?.overall_requests(workspace) == OverallRequests::None {
        return Err(error_body(
            ErrorCode::Unsupported,
            format!(
                "workspace {workspace:?} takes no requests from the overall assistant: its \
                 [workspace.assistant] overall_requests is \"none\", or it takes no requests"
            ),
        ));
    }
    Ok(())
}

/// Blocking. Claims the approval at `rev` under the item file's lock.
fn claim(config: &Config, f: &Found, rev: u64, by: By, at: u64) -> Result<Claim, ErrorBody> {
    with_items(config, &f.task, |items| {
        let mut a =
            approval::get(items, &f.item.id, &f.hash).unwrap_or_else(|| Approval::new(&f.hash));
        let item = stored(items, &f.item.id)?.clone();
        if a.done() {
            return Ok(Claim::Done(Box::new(Found {
                item,
                approval: Some(a),
                ..f.clone()
            })));
        }
        if !matches!(item.state, ItemState::Open | ItemState::Later) {
            return Err(bad(format!("Inbox item {} is already closed", item.id)).into());
        }
        if a.since
            .is_some_and(|s| at.saturating_sub(s) < APPROVAL_LIMIT)
        {
            return Err(error_body(
                ErrorCode::Conflict,
                format!("Inbox item {} is being approved; wait for it", item.id),
            )
            .into());
        }
        a.since = Some(at);
        a.rev = Some(rev);
        if by == By::Host {
            a.auto_at = Some(at);
        }
        approval::put(items, &f.item.id, a);
        Ok(Claim::Go)
    })
}

/// Blocking. Records what the approval made and answers the item `approve`.
fn finish(config: &Config, f: &Found, ids: Ids, by: By, at: u64) -> Result<InboxItem, ErrorBody> {
    with_items(config, &f.task, |items| {
        let mut a =
            approval::get(items, &f.item.id, &f.hash).unwrap_or_else(|| Approval::new(&f.hash));
        a.since = None;
        (a.capture, a.work, a.request) = (ids.capture, ids.work, ids.request);
        if by == By::Host {
            a.auto_at.get_or_insert(at);
        }
        approval::put(items, &f.item.id, a);
        let item = stored(items, &f.item.id)?;
        item.state = ItemState::Answered;
        item.answer = Some(InboxAction::Approve);
        item.answered_at = Some(at);
        item.until = None;
        if by == By::Host {
            let ws = item.delegate.as_ref().map_or("", |d| d.workspace.as_str());
            item.summary = Some(format!(
                "Sent without review: workspace {ws:?} sets overall_requests = \"auto\"."
            ));
        }
        item.answers = answers(item);
        Ok(shown(item))
    })
}

/// Blocking. Ends a claim that made nothing. A host refusal keeps the item
/// for review with the reason, and the host does not try it again.
pub fn release(config: &Config, f: &Found, refusal: Option<&str>, at: u64) {
    let released = with_items(config, &f.task, |items| {
        let mut a =
            approval::get(items, &f.item.id, &f.hash).unwrap_or_else(|| Approval::new(&f.hash));
        a.since = None;
        if refusal.is_some() {
            a.auto_at = None;
            a.auto_refused = Some(at);
        }
        approval::put(items, &f.item.id, a);
        if let Some(why) = refusal {
            stored(items, &f.item.id)?.summary =
                Some(format!("Not sent automatically: {why} Approve to send it."));
        }
        Ok(())
    });
    if let Err(e) = released {
        tracing::warn!(item = %f.item.id, error = %e.message, "cannot release a proposal's approval");
    }
}

/// Blocking. Closes an item whose action can no longer run, with `why`.
fn resolve(config: &Config, f: &Found, why: &str) -> Result<InboxItem, ErrorBody> {
    with_items(config, &f.task, |items| {
        let mut a =
            approval::get(items, &f.item.id, &f.hash).unwrap_or_else(|| Approval::new(&f.hash));
        a.since = None;
        approval::put(items, &f.item.id, a);
        let item = stored(items, &f.item.id)?;
        item.state = ItemState::Resolved;
        item.summary = Some(why.to_owned());
        item.until = None;
        item.answers = answers(item);
        Ok(shown(item))
    })
}

fn stored<'a>(items: &'a mut TaskItems, id: &str) -> Result<&'a mut InboxItem, Fail> {
    items
        .items
        .iter_mut()
        .find(|s| s.item.id == id)
        .map(|s| &mut s.item)
        .ok_or_else(|| bad(format!("Inbox item {id} was removed")).into())
}

fn with_items<T>(
    config: &Config,
    task: &str,
    change: impl FnOnce(&mut TaskItems) -> Result<T, Fail>,
) -> Result<T, ErrorBody> {
    let dir = config
        .state_dir
        .as_ref()
        .ok_or_else(|| error_body(ErrorCode::Internal, "the state directory went away"))?;
    let store = ItemStore::open(dir).map_err(|e| state_error(&e))?;
    store.update(task, change).map_err(ErrorBody::from)
}

fn bad(message: impl Into<String>) -> ErrorBody {
    error_body(ErrorCode::BadRequest, message.into())
}
