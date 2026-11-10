//! `handoff` Inbox items (ADR 0051 §3): one per request, version and step,
//! in the asking step's run file, keyed so a second scan makes none. The
//! purpose is in the item, which is the user's; never in a log.
use muxa_protocol::{
    AssistantRequest, ErrorBody, ErrorCode, HandoffProposal, InboxItem, ItemKind, ItemState,
    WorkItem,
};

use crate::assistant::{shelf, state_error};
use crate::convert::error_body;
use crate::items::produce::{MAX_QUESTION, one_line};
use crate::items::store::{ItemStore, StoredItem, TaskItems, new_id};
use crate::routines::store::StateError;
use crate::task_records::Config;

/// The item's key in its file: the step whose result asks.
pub fn key(request: &str, version: u64, step: u32) -> String {
    format!("handoff:{request}:{version}:{step}")
}

/// Blocking. The item of `q`'s step `step` in `task`'s file, unless one was
/// made before (kept or deleted). Its new id, or None.
pub fn make(
    config: &Config,
    task: &str,
    q: &AssistantRequest,
    step: u32,
    summary: Option<String>,
) -> Result<Option<String>, ErrorBody> {
    let Some(proposal) = proposal(q, step) else {
        return Ok(None);
    };
    let title = shelf::<WorkItem>(config)?
        .get(&q.work)
        .map_err(|e| state_error(&e))?
        .map(|w| w.title)
        .unwrap_or_default();
    let key = key(&q.id, q.version, step);
    let at = crate::records::now();
    store(config)?
        .update::<_, StateError>(task, |items| {
            if items.has_key(&key) {
                return Ok(None);
            }
            let item = item_for(proposal, &q.workspace, &title, summary, new_id()?, at);
            let id = item.id.clone();
            items.items.push(StoredItem { item, key });
            Ok(Some(id))
        })
        .map_err(|e| state_error(&e))
}

/// What approve starts: the step after `step`.
fn proposal(q: &AssistantRequest, step: u32) -> Option<HandoffProposal> {
    let s = q.steps.iter().find(|s| s.step == step)?;
    let h = s.handoff.as_ref()?;
    Some(HandoffProposal {
        request: q.id.clone(),
        version: q.version,
        step,
        work: q.work.clone(),
        from: s.worker.clone(),
        worker: h.worker.clone(),
        purpose: h.purpose.clone(),
    })
}

fn item_for(
    p: HandoffProposal,
    workspace: &str,
    title: &str,
    summary: Option<String>,
    id: String,
    at: u64,
) -> InboxItem {
    let question = format!("{} asks {} to: {}", p.from, p.worker, p.purpose);
    let mut item = InboxItem {
        id,
        kind: ItemKind::Handoff,
        state: ItemState::Open,
        question: one_line(&question, MAX_QUESTION),
        workspace: workspace.to_owned(),
        title: one_line(title, MAX_QUESTION),
        created_at: at,
        read_at: None,
        later_at: None,
        until: None,
        protected_at: None,
        answered_at: None,
        answer: None,
        summary,
        task: None,
        session: None,
        pane: None,
        prompt: None,
        attachments: Default::default(),
        routine: None,
        proposal: None,
        sort: None,
        delegate: None,
        hypothesis: None,
        handoff: Some(p),
        answers: Vec::new(),
    };
    item.answers = crate::items::answers(&item);
    item
}

/// Blocking. Every open `handoff` item in `task`'s file becomes `resolved`
/// with `why`. A file holds the items of one step only.
pub fn resolve(config: &Config, task: &str, why: &str) -> Result<usize, ErrorBody> {
    store(config)?
        .update::<_, StateError>(task, |items| Ok(close(items, why)))
        .map_err(|e| state_error(&e))
}

fn close(items: &mut TaskItems, why: &str) -> usize {
    let mut n = 0;
    for s in items
        .items
        .iter_mut()
        .filter(|s| s.key.starts_with("handoff:"))
    {
        if matches!(s.item.state, ItemState::Open | ItemState::Later) {
            s.item.state = ItemState::Resolved;
            s.item.summary = Some(why.to_owned());
            s.item.until = None;
            s.item.answers = crate::items::answers(&s.item);
            n += 1;
        }
    }
    n
}

fn store(config: &Config) -> Result<ItemStore, ErrorBody> {
    let dir = config
        .state_dir
        .as_ref()
        .ok_or_else(|| error_body(ErrorCode::Internal, "no state directory"))?;
    ItemStore::open(dir).map_err(|e| state_error(&e))
}
