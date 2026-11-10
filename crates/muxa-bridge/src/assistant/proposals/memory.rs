//! `update-inbox approve` on a `memory` item (ADR 0053 §3): the Task's text
//! is appended once to `.muxa/WORKSPACE.md` under its heading. Approve binds
//! to the hash of the text the client showed whole; a second approve, also
//! after a crash between the rename and the answer, appends nothing more.
//! Past the 16 KiB cap the item stays open. Logs carry ids and sizes.
use std::sync::Arc;

use muxa_protocol::{ErrorBody, ErrorCode, InboxAction, ItemKind, ItemState};

use super::approval::{self, Approval};
use super::approve::{Approved, Found, bad, stored, with_items};
use crate::assistant::blocking;
use crate::assistant::notes::Appended;
use crate::convert::error_body;
use crate::items::{answers, shown};
use crate::records::now;
use crate::task_records::{Config, Context};
use crate::workspace_memory::{self, approvable};

pub(super) async fn approve(cx: &Context, f: Found) -> Result<Approved, ErrorBody> {
    let config = Arc::clone(&cx.records);
    blocking(move || append(&config, &f, now())).await
}

/// A client that cannot show the text whole sends no hash, so it never
/// approves; a hash of other text is refused too.
pub(super) fn hash_shown(f: &Found, hash: Option<&str>) -> Result<(), ErrorBody> {
    let Some(m) = f
        .item
        .memory
        .as_ref()
        .filter(|_| f.item.kind == ItemKind::Memory)
    else {
        return Ok(());
    };
    if hash == Some(m.hash.as_str()) {
        return Ok(());
    }
    tracing::info!(item = %f.item.id, sent = hash.is_some(), "memory approval refused: no matching hash");
    Err(bad(
        "this app cannot show the whole memory text; update the app or use muxa-cli",
    ))
}

/// Blocking. Appends under the workspace's lock, then answers the item.
fn append(config: &Config, f: &Found, at: u64) -> Result<Approved, ErrorBody> {
    let p = f
        .item
        .memory
        .as_ref()
        .ok_or_else(|| bad("the item has no memory"))?;
    if !approvable(&p.text) {
        return Err(bad(
            "this text cannot be added: it is over 4 KiB or has control characters; copy what \
             you need by hand",
        ));
    }
    if workspace_memory::hash(&p.task, &p.title, &p.text) != p.hash {
        return Err(conflict(
            "the item's text does not match its hash; nothing was written",
        ));
    }
    let ws = &f.item.workspace;
    let root = config
        .load_workspaces()?
        .get(ws)
        .map(|w| w.root.clone())
        .ok_or_else(|| {
            conflict(format!(
                "workspace {ws:?} is not on this host now; the item stays open"
            ))
        })?;
    let appended = workspace_memory::append(config, &root, p, at).map_err(|e| {
        tracing::info!(item = %f.item.id, task = %p.task, error = %e, "memory append refused");
        conflict(e.to_string())
    })?;
    match appended {
        Appended::Added { size } => {
            tracing::info!(item = %f.item.id, task = %p.task, workspace = %ws, size, hash = %p.hash, "memory approved and appended")
        }
        Appended::Present => {
            tracing::info!(item = %f.item.id, task = %p.task, "memory approved; appended before")
        }
    }
    let item = with_items(config, &f.task, |items| {
        let mut a =
            approval::get(items, &f.item.id, &f.hash).unwrap_or_else(|| Approval::new(&f.hash));
        a.memory_at.get_or_insert(at);
        approval::put(items, &f.item.id, a);
        let item = stored(items, &f.item.id)?;
        if matches!(item.state, ItemState::Open | ItemState::Later) {
            item.state = ItemState::Answered;
            item.answer = Some(InboxAction::Approve);
            item.answered_at = Some(at);
            item.until = None;
        }
        item.answers = answers(item);
        Ok(shown(item))
    })?;
    Ok(Approved {
        made: None,
        item,
        error: None,
    })
}

fn conflict(message: impl Into<String>) -> ErrorBody {
    error_body(ErrorCode::Conflict, message)
}
