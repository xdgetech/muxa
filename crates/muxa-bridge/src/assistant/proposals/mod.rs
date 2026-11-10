//! The overall assistant's proposals (ADR 0049 §4): after a plan is stored,
//! the daemon's tick makes each of its `sort` and `delegate` proposals an
//! Inbox item, and `update-inbox approve` runs the action once. A delegate
//! to a workspace with `overall_requests = "auto"` is approved by the host
//! itself: the item is kept, answered `approve`, so the user sees what was
//! sent. Sorting is always reviewed. A `write` item's approve makes one
//! Task of the workspace's writer (ADR 0052 §3, in `write.rs`).
mod approval;
pub mod approve;
mod handoff;
pub mod inbox;
mod write;

use std::sync::Arc;

use muxa_protocol::{ErrorBody, Message};

use crate::task_records::Context;
use approve::{Approved, By, Made};

/// `update-inbox approve` on a `sort`, `delegate`, `handoff` or `write` item:
/// `capture-saved`, `work-saved` or `task-saved` with what it made, then
/// `inbox-saved`, and the error for `result`. None for any other item.
pub async fn answer(
    cx: &Context,
    id: u64,
    entry: &str,
) -> Option<(Vec<Message>, Option<ErrorBody>)> {
    let (config, item) = (Arc::clone(&cx.records), entry.to_owned());
    let found = match super::blocking(move || approve::find(&config, &item)).await {
        Ok(Some(found)) => found,
        Ok(None) => return None,
        Err(e) => return Some((Vec::new(), Some(e))),
    };
    Some(match approve::approve(cx, found, By::User).await {
        Ok(approved) => {
            let error = approved.error.clone();
            (messages(id, approved), error)
        }
        Err(e) => (Vec::new(), Some(e)),
    })
}

fn messages(id: u64, approved: Approved) -> Vec<Message> {
    let mut out = Vec::new();
    match approved.made {
        Some(Made::Sorted(entry, work)) => out.push(Message::CaptureSaved {
            capture: id,
            entry,
            work: work.map(Box::new),
            request: None,
        }),
        Some(Made::Asked(entry, request)) => out.push(Message::WorkSaved {
            work: id,
            entry,
            request: Some(Box::new(request)),
        }),
        Some(Made::Task(record)) => out.push(Message::TaskSaved {
            task: id,
            record: crate::convert::task_record(*record),
        }),
        None => {}
    }
    out.push(Message::InboxSaved {
        inbox: id,
        entry: None,
        item: Some(Box::new(approved.item)),
    });
    out
}

/// The daemon's tick: new plans' proposals become items, then the host
/// sends the `auto` ones it may, one at a time.
pub async fn tick(cx: &Context) {
    let config = Arc::clone(&cx.records);
    let autos = tokio::task::spawn_blocking(move || inbox::publish(&config))
        .await
        .unwrap_or_else(|e| {
            tracing::warn!(error = %e, "the proposal scan stopped");
            Vec::new()
        });
    for a in autos {
        send(cx, a).await;
    }
}

async fn send(cx: &Context, a: inbox::Auto) {
    let (config, item) = (Arc::clone(&cx.records), a.item.clone());
    let checked =
        super::blocking(move || inbox::may_send(&config, &a, crate::records::now())).await;
    let found = match checked {
        Ok(Some(found)) => found,
        Ok(None) => return,
        Err(e) => {
            tracing::warn!(item, error = %e.message, "cannot check an auto request");
            return;
        }
    };
    match approve::approve(cx, found, By::Host).await {
        Ok(_) => tracing::info!(item, "delegate request sent without review"),
        Err(e) => {
            tracing::info!(item, code = ?e.code, "delegate request held for review")
        }
    }
}

#[cfg(test)]
mod tests;
