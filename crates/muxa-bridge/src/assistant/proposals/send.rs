//! `update-inbox reply` on a `result` item (Send) or a `write` item (Revise),
//! ADR 0058 §3 and §4: a new version of the request, whose first step is the
//! member who wrote the Result and whose purpose is the user's text. Keyed
//! by the item's id and hash through the one `Approval`, so a second reply
//! returns the version the first made. The text is never logged.
use std::sync::Arc;

use muxa_protocol::{
    AssistantRequest, ErrorBody, ErrorCode, InboxAction, InboxItem, StepBy, WorkItem,
};

use super::approve::{Approved, By, Claim, Found, Ids, Made, bad, claim, finish_as, release};
use crate::assistant::ask::{self, Ask};
use crate::assistant::follow_up::FollowUp;
use crate::assistant::{NO_DAEMON, blocking, shelf, state_error};
use crate::convert::error_body;
use crate::items::answer::MAX_TEXT;
use crate::records::now;
use crate::task_records::{Config, Context};

/// The Result the reply continues, and who wrote it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Target {
    pub request: String,
    pub follows: FollowUp,
    /// None: the workspace's delegate, or its lead.
    pub worker: Option<String>,
}

/// Pure. What `item` asks again; None for an item that takes no Send.
pub(super) fn target(item: &InboxItem, q: Option<&AssistantRequest>) -> Option<Target> {
    if let Some(r) = &item.result {
        return Some(Target {
            request: r.request.clone(),
            follows: FollowUp {
                version: r.version,
                step: r.step,
                write: None,
            },
            worker: r.worker.clone(),
        });
    }
    let w = item.write.as_ref()?;
    let step = w.step.unwrap_or(1);
    let index = q.and_then(|q| {
        q.results
            .iter()
            .find(|r| r.version == w.version && r.step.unwrap_or(1) == step)?
            .writes
            .iter()
            .position(|x| x.hash == w.hash)
    });
    Some(Target {
        request: w.request.clone(),
        follows: FollowUp {
            version: w.version,
            step,
            write: index,
        },
        worker: w.worker.clone(),
    })
}

pub(super) async fn send(cx: &Context, f: Found, text: String) -> Result<Approved, ErrorBody> {
    if text.trim().is_empty() || text.len() > MAX_TEXT || text.contains('\0') {
        return Err(bad(format!(
            "the text is 1 to {MAX_TEXT} bytes, without NUL"
        )));
    }
    if let Some(a) = f.approval.as_ref().filter(|a| a.done()) {
        let (work, request) = (a.work.clone(), a.request.clone());
        let (Some(work), Some(request)) = (work, request) else {
            return Err(bad(format!("Inbox item {} was approved", f.item.id)));
        };
        let config = Arc::clone(&cx.records);
        let item = crate::items::shown(&f.item);
        return blocking(move || again(&config, &work, &request, item)).await;
    }
    if !cx.daemon {
        return Err(error_body(ErrorCode::Unsupported, NO_DAEMON));
    }
    let (config, found, at) = (Arc::clone(&cx.records), f.clone(), now());
    let (claimed, asked) = blocking(move || prepare(&config, &found, text, at)).await?;
    if let Claim::Done(done) = claimed {
        let a = done.approval.clone().unwrap_or_default();
        let (Some(work), Some(request)) = (a.work, a.request) else {
            return Err(bad(format!("Inbox item {} was approved", done.item.id)));
        };
        let config = Arc::clone(&cx.records);
        let item = crate::items::shown(&done.item);
        return blocking(move || again(&config, &work, &request, item)).await;
    }
    let config = Arc::clone(&cx.records);
    let worker = asked.worker.clone();
    match ask::ask(cx, asked).await {
        Ok((w, q, error)) => {
            let ids = Ids {
                capture: None,
                work: Some(w.id.clone()),
                request: Some(q.id.clone()),
            };
            let saved = f.clone();
            let item = blocking(move || {
                finish_as(&config, &saved, ids, By::User, now(), InboxAction::Reply)
            })
            .await?;
            tracing::info!(item = %f.item.id, kind = ?f.item.kind, request = %q.id, version = q.version, worker = ?worker, "result answered with a new version");
            Ok(Approved {
                made: Some(Made::Asked(w, q)),
                item,
                error,
            })
        }
        Err(e) => {
            let released = blocking(move || {
                release(&config, &f, None, now());
                Ok(())
            })
            .await;
            if let Err(r) = released {
                tracing::warn!(error = %r.message, "cannot release a reply's claim");
            }
            Err(e)
        }
    }
}

/// Blocking. The claim at the Work item's `rev`, and the request to make.
fn prepare(config: &Config, f: &Found, text: String, at: u64) -> Result<(Claim, Ask), ErrorBody> {
    let requests = shelf::<AssistantRequest>(config)?;
    let first = target(&f.item, None).ok_or_else(|| bad("the item takes no reply"))?;
    let q = requests
        .get(&first.request)
        .map_err(|e| state_error(&e))?
        .ok_or_else(|| bad(format!("no request {}", first.request)))?;
    let t = target(&f.item, Some(&q)).unwrap_or(first);
    let current = shelf::<WorkItem>(config)?
        .get(&q.work)
        .map_err(|e| state_error(&e))?
        .ok_or_else(|| bad(format!("no Work item {}", q.work)))?
        .rev;
    // A resend of the rev an earlier attempt asked at finds its version.
    let stored = f.approval.as_ref().and_then(|a| a.rev);
    let rev = ask::asked_rev(config, &q.work, &q.workspace)?
        .filter(|r| Some(*r) == stored)
        .unwrap_or(current);
    let claimed = claim(config, f, rev, By::User, at)?;
    let asked = Ask {
        work: q.work.clone(),
        rev,
        scope: q.scope,
        workspace: Some(q.workspace.clone()),
        purpose: Some(text),
        // A request without steps has no staff to name.
        worker: t.worker.filter(|_| !q.steps.is_empty()),
        asker: StepBy::User,
        follows: Some(t.follows),
    };
    Ok((claimed, asked))
}

/// Blocking. What the first reply made, read again.
fn again(
    config: &Config,
    work: &str,
    request: &str,
    item: InboxItem,
) -> Result<Approved, ErrorBody> {
    let (w, q) = ask::reply(config, work, request)?;
    Ok(Approved {
        made: Some(Made::Asked(w, q)),
        item,
        error: None,
    })
}

#[cfg(test)]
mod tests {
    use muxa_protocol::{ItemKind, ResultReview, WriteProposal};
    use serde_json::json;

    use super::*;

    fn item(kind: ItemKind) -> InboxItem {
        let mut i = crate::items::store::tests_item("i-00000001", muxa_protocol::ItemState::Open);
        i.kind = kind;
        i
    }

    #[test]
    fn send_continues_the_items_result_and_revise_names_the_write() {
        let mut r = item(ItemKind::Result);
        r.result = Some(ResultReview {
            task: None,
            request: "q-0000000a".into(),
            version: 2,
            step: 2,
            work: "w-0000000b".into(),
            worker: Some("strategist".into()),
            findings: Vec::new(),
            open_questions: Vec::new(),
            next: Vec::new(),
            files: Vec::new(),
            handoff: None,
        });
        let t = target(&r, None).unwrap();
        assert_eq!(
            (t.request.as_str(), t.follows.version, t.follows.step),
            ("q-0000000a", 2, 2)
        );
        assert_eq!(t.worker.as_deref(), Some("strategist"));

        let mut w = item(ItemKind::Write);
        w.write = Some(WriteProposal {
            request: "q-0000000a".into(),
            version: 1,
            step: None,
            worker: None,
            action: "Post".into(),
            target: "slack:#x".into(),
            content: "c".into(),
            hash: "sha256:b".into(),
        });
        let q: AssistantRequest = serde_json::from_value(json!({
            "id": "q-0000000a", "work": "w-0000000b", "version": 2, "workspace": "lisk",
            "scope": "draft", "delivery": "resulted", "deliveryAt": 1,
            "createdAt": 1, "updatedAt": 1,
            "results": [{"version": 1, "at": 1, "summary": "s", "writes": [
                {"action": "A", "target": "t", "content": "c", "hash": "sha256:a"},
                {"action": "Post", "target": "slack:#x", "content": "c", "hash": "sha256:b"},
            ]}],
        }))
        .unwrap();
        let t = target(&w, Some(&q)).unwrap();
        assert_eq!(
            (t.follows.version, t.follows.step, t.follows.write),
            (1, 1, Some(1))
        );
        assert_eq!(t.worker, None);
        assert!(target(&item(ItemKind::Review), None).is_none());
    }
}
