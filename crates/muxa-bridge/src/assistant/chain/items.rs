//! `result` Inbox items (ADR 0058 §1): one per request, version and step,
//! in the step's run file, keyed so a second scan makes none. It carries the
//! Result and offers the step's hand-off (ADR 0051 §3). The Result is the
//! user's; never in a log. `handoff` items from before ADR 0058 are closed
//! here too.
use muxa_protocol::{
    AssistResult, AssistantRequest, ErrorBody, ErrorCode, InboxItem, ItemKind, ItemState,
    ResultReview, SourceLink, WorkItem,
};

use crate::assistant::{shelf, state_error};
use crate::convert::error_body;
use crate::items::produce::{MAX_QUESTION, one_line};
use crate::items::store::{ItemStore, StoredItem, TaskItems, new_id};
use crate::routines::store::StateError;
use crate::task_records::Config;

/// Characters of a finding's first line the item keeps.
const MAX_FINDING: usize = 200;
/// Links the item lists, as a Task's item keeps (`PROTOCOL.md` "Inbox items").
const MAX_LINKS: usize = 10;
/// Bytes of a summary an item keeps.
const MAX_SUMMARY: usize = 4096;

/// The item's key in its file: the step whose Result it is.
pub fn key(request: &str, version: u64, step: u32) -> String {
    format!("result:{request}:{version}:{step}")
}

/// Blocking. The item of `q`'s step `step` in `task`'s file, unless one was
/// made before (kept or deleted). `handoff`: it offers the step's hand-off.
/// `held` goes before the summary. Its new id, or None.
pub fn make(
    config: &Config,
    task: &str,
    q: &AssistantRequest,
    step: u32,
    handoff: bool,
    held: Option<String>,
) -> Result<Option<String>, ErrorBody> {
    let Some(result) = q
        .results
        .iter()
        .find(|r| r.version == q.version && r.step.unwrap_or(1) == step)
    else {
        return Ok(None);
    };
    let title = shelf::<WorkItem>(config)?
        .get(&q.work)
        .map_err(|e| state_error(&e))?
        .map(|w| w.title)
        .unwrap_or_default();
    let review = review(q, step, result, handoff);
    let key = key(&q.id, q.version, step);
    let at = crate::records::now();
    store(config)?
        .update::<_, StateError>(task, |items| {
            if items.has_key(&key) {
                return Ok(None);
            }
            let made = Made {
                review,
                summary: summary(&result.summary, held.as_deref()),
                links: links(result),
                workspace: &q.workspace,
                title: &title,
            };
            let item = made.item(new_id()?, at);
            let id = item.id.clone();
            items.items.push(StoredItem { item, key });
            Ok(Some(id))
        })
        .map_err(|e| state_error(&e))
}

/// Pure. What the user reviews: the Result's lists, and the hand-off.
pub fn review(
    q: &AssistantRequest,
    step: u32,
    result: &AssistResult,
    handoff: bool,
) -> ResultReview {
    let s = q.steps.iter().find(|s| s.step == step);
    ResultReview {
        request: q.id.clone(),
        version: q.version,
        step,
        work: q.work.clone(),
        worker: s
            .map(|s| s.worker.clone())
            .or_else(|| result.worker.clone()),
        findings: result
            .findings
            .iter()
            .map(|f| one_line(f.text.lines().next().unwrap_or_default(), MAX_FINDING))
            .collect(),
        open_questions: result.open_questions.clone(),
        next: result.next.clone(),
        files: result.files.clone(),
        handoff: s.and_then(|s| s.handoff.clone()).filter(|_| handoff),
    }
}

fn summary(text: &str, held: Option<&str>) -> Option<String> {
    let whole = match held {
        Some(h) => format!("{h}\n\n{text}"),
        None => text.to_owned(),
    };
    let mut keep = whole.len().min(MAX_SUMMARY);
    while !whole.is_char_boundary(keep) {
        keep -= 1;
    }
    Some(whole[..keep].to_owned()).filter(|s| !s.is_empty())
}

/// The findings' source URLs, each once.
fn links(result: &AssistResult) -> Vec<SourceLink> {
    let mut out: Vec<SourceLink> = Vec::new();
    for s in result.findings.iter().flat_map(|f| &f.sources) {
        let Some(url) = s
            .url
            .as_ref()
            .filter(|u| u.starts_with("https://") || u.starts_with("http://"))
        else {
            continue;
        };
        if out.len() == MAX_LINKS || out.iter().any(|l| &l.url == url) {
            continue;
        }
        out.push(SourceLink {
            url: url.clone(),
            title: s.title.clone(),
        });
    }
    out
}

struct Made<'a> {
    review: ResultReview,
    summary: Option<String>,
    links: Vec<SourceLink>,
    workspace: &'a str,
    title: &'a str,
}

impl Made<'_> {
    fn question(&self) -> String {
        let who = self.review.worker.as_deref().unwrap_or("The assistant");
        match &self.review.handoff {
            Some(h) => format!("{who} asks {} to: {}", h.worker, h.purpose),
            None => {
                let first = self
                    .summary
                    .as_deref()
                    .and_then(|s| s.lines().find(|l| !l.trim().is_empty()))
                    .unwrap_or_default();
                format!("{who} finished: {first}")
            }
        }
    }

    fn item(self, id: String, at: u64) -> InboxItem {
        let question = one_line(&self.question(), MAX_QUESTION);
        let mut item = InboxItem {
            result: Some(self.review),
            memory: None,
            write: None,
            id,
            kind: ItemKind::Result,
            state: ItemState::Open,
            question,
            workspace: self.workspace.to_owned(),
            title: one_line(self.title, MAX_QUESTION),
            created_at: at,
            read_at: None,
            later_at: None,
            until: None,
            protected_at: None,
            answered_at: None,
            answer: None,
            summary: self.summary,
            task: None,
            session: None,
            pane: None,
            prompt: None,
            attachments: muxa_protocol::Attachments {
                links: self.links,
                ..Default::default()
            },
            routine: None,
            proposal: None,
            sort: None,
            delegate: None,
            hypothesis: None,
            handoff: None,
            answers: Vec::new(),
        };
        item.answers = crate::items::answers(&item);
        item
    }
}

/// Blocking. Every open `result` or `handoff` item in `task`'s file becomes
/// `resolved` with `why`. A file holds the items of one step only.
pub fn resolve(config: &Config, task: &str, why: &str) -> Result<usize, ErrorBody> {
    store(config)?
        .update::<_, StateError>(task, |items| Ok(close(items, why)))
        .map_err(|e| state_error(&e))
}

/// Blocking. A cancel ended the chain: an open `result` item stays open
/// without its hand-off, with `why` before its summary; a `handoff` item is
/// resolved.
pub fn end(config: &Config, task: &str, why: &str) -> Result<usize, ErrorBody> {
    store(config)?
        .update::<_, StateError>(task, |items| {
            let mut n = close_handoffs(items, why);
            for s in open(items, "result:") {
                let Some(r) = s.item.result.as_mut().filter(|r| r.handoff.is_some()) else {
                    continue;
                };
                r.handoff = None;
                s.item.summary = summary(s.item.summary.as_deref().unwrap_or_default(), Some(why));
                s.item.answers = crate::items::answers(&s.item);
                n += 1;
            }
            Ok(n)
        })
        .map_err(|e| state_error(&e))
}

fn close(items: &mut TaskItems, why: &str) -> usize {
    let mut n = close_handoffs(items, why);
    for s in open(items, "result:") {
        // The Result stays readable in the item's history.
        let kept = summary(s.item.summary.as_deref().unwrap_or_default(), Some(why));
        resolved(&mut s.item, why);
        s.item.summary = kept;
        n += 1;
    }
    n
}

fn close_handoffs(items: &mut TaskItems, why: &str) -> usize {
    let mut n = 0;
    for s in open(items, "handoff:") {
        resolved(&mut s.item, why);
        n += 1;
    }
    n
}

fn open<'a>(items: &'a mut TaskItems, prefix: &'a str) -> impl Iterator<Item = &'a mut StoredItem> {
    items.items.iter_mut().filter(move |s| {
        s.key.starts_with(prefix) && matches!(s.item.state, ItemState::Open | ItemState::Later)
    })
}

fn resolved(item: &mut InboxItem, why: &str) {
    item.state = ItemState::Resolved;
    item.summary = Some(why.to_owned());
    item.until = None;
    item.answers = crate::items::answers(item);
}

fn store(config: &Config) -> Result<ItemStore, ErrorBody> {
    let dir = config
        .state_dir
        .as_ref()
        .ok_or_else(|| error_body(ErrorCode::Internal, "no state directory"))?;
    ItemStore::open(dir).map_err(|e| state_error(&e))
}
