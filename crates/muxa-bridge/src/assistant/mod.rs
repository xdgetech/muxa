//! Captures, Work items and assistant requests (ADR 0046): the user's input
//! and unfinished work, kept apart from Tasks and Inbox items.
pub mod capture;
pub mod store;

use std::collections::HashSet;
use std::sync::Arc;

use muxa_protocol::{
    AssistantRequest, Capture, CaptureState, ErrorBody, ErrorCode, Message, Request, WorkItem,
    WorkState,
};

use crate::convert::error_body;
use crate::routines::store::StateError;
use crate::task_records::{Config, Context};
use store::{Kind, Shelf};

/// Done or withdrawn Work items in one list.
const MAX_CLOSED: usize = 100;
/// Unsorted Captures in one list.
const MAX_CAPTURES: usize = 200;

/// Answers a `work` level 8 request: its saved message, and its error.
pub async fn handle(cx: &Context, req: Request) -> (Vec<Message>, Option<ErrorBody>) {
    match req {
        Request::Capture {
            id,
            client_id,
            text,
            source,
            workspace,
            assist,
        } => {
            let config = Arc::clone(&cx.records);
            let wants_assist = assist.is_some();
            let new = capture::NewCapture {
                client_id,
                text,
                source,
                workspace,
                assist,
            };
            match blocking(move || capture::save(&config, new)).await {
                Ok((entry, _)) => {
                    let saved = Message::CaptureSaved {
                        capture: id,
                        entry,
                        work: None,
                        request: None,
                    };
                    // The input is kept; only the start is refused (ADR 0046 §8).
                    let error = wants_assist.then(|| {
                        error_body(
                            ErrorCode::Unsupported,
                            "the Capture is saved; this host cannot ask the assistant yet",
                        )
                    });
                    (vec![saved], error)
                }
                Err(e) => (Vec::new(), Some(e)),
            }
        }
        _ => (
            Vec::new(),
            Some(error_body(
                ErrorCode::Unsupported,
                "not an assistant request",
            )),
        ),
    }
}

/// What `inbox-list` adds at level 8.
#[derive(Debug, Default)]
pub struct Listing {
    /// In the order to show them.
    pub work: Vec<WorkItem>,
    /// Unsorted only, newest first.
    pub captures: Vec<Capture>,
    /// Those of the listed Work items.
    pub requests: Vec<AssistantRequest>,
}

/// Blocking. Empty without a state directory.
pub fn list(config: &Config) -> Result<Listing, ErrorBody> {
    if !config.state_dir.as_ref().is_some_and(|d| d.is_dir()) {
        return Ok(Default::default());
    }
    let work = order(
        shelf::<WorkItem>(config)?
            .all()
            .map_err(|e| state_error(&e))?,
    );
    let mut captures: Vec<Capture> = shelf::<Capture>(config)?
        .all()
        .map_err(|e| state_error(&e))?
        .into_iter()
        .filter(|c| c.state == CaptureState::Received)
        .collect();
    captures.sort_by(|a, b| b.created_at.cmp(&a.created_at).then(b.id.cmp(&a.id)));
    captures.truncate(MAX_CAPTURES);
    let wanted: HashSet<&str> = work
        .iter()
        .flat_map(|w| w.requests.iter().map(String::as_str))
        .collect();
    let requests = shelf::<AssistantRequest>(config)?
        .all()
        .map_err(|e| state_error(&e))?
        .into_iter()
        .filter(|r| wanted.contains(r.id.as_str()))
        .collect();
    Ok(Listing {
        work,
        captures,
        requests,
    })
}

/// Open items, newest `createdAt` first; then the ones moved down with
/// Later, in the order moved; then at most [`MAX_CLOSED`] closed ones,
/// newest `updatedAt` first.
fn order(items: Vec<WorkItem>) -> Vec<WorkItem> {
    let closed = |w: &WorkItem| matches!(w.state, WorkState::Done | WorkState::Withdrawn);
    let (mut done, rest): (Vec<_>, Vec<_>) = items.into_iter().partition(closed);
    let (mut later, mut open): (Vec<_>, Vec<_>) =
        rest.into_iter().partition(|w| w.later_at.is_some());
    open.sort_by(|a, b| b.created_at.cmp(&a.created_at).then(b.id.cmp(&a.id)));
    later.sort_by(|a, b| a.later_at.cmp(&b.later_at).then(a.id.cmp(&b.id)));
    done.sort_by(|a, b| b.updated_at.cmp(&a.updated_at).then(b.id.cmp(&a.id)));
    done.truncate(MAX_CLOSED);
    open.into_iter().chain(later).chain(done).collect()
}

/// Opens one kind's folder; refused without a state directory.
fn shelf<T: Kind>(config: &Config) -> Result<Shelf<T>, ErrorBody> {
    let dir = config.state_dir.as_ref().ok_or_else(|| {
        error_body(
            ErrorCode::Unsupported,
            "this bridge keeps no records: HOME is not set; pass --state-dir",
        )
    })?;
    Shelf::open(dir).map_err(|e| state_error(&e))
}

/// The error a client sees; the detail goes to the log. Neither holds text.
fn state_error(e: &StateError) -> ErrorBody {
    tracing::warn!(error = %e, "assistant store");
    let code = match e {
        StateError::Lock(crate::records::StoreError::Busy(_)) => ErrorCode::Conflict,
        _ => ErrorCode::Internal,
    };
    error_body(code, e.to_string())
}

async fn blocking<T: Send + 'static>(
    f: impl FnOnce() -> Result<T, ErrorBody> + Send + 'static,
) -> Result<T, ErrorBody> {
    tokio::task::spawn_blocking(f).await.unwrap_or_else(|_| {
        Err(error_body(
            ErrorCode::Internal,
            "the assistant store stopped",
        ))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(
        id: &str,
        state: WorkState,
        created: u64,
        updated: u64,
        later: Option<u64>,
    ) -> WorkItem {
        WorkItem {
            id: id.into(),
            rev: 1,
            title: "t".into(),
            state,
            created_at: created,
            updated_at: updated,
            workspace: None,
            later_at: later,
            merged_into: None,
            captures: Vec::new(),
            requests: Vec::new(),
            tasks: Vec::new(),
        }
    }

    #[test]
    fn open_items_come_first_then_later_then_closed() {
        use WorkState::*;
        let listed = order(vec![
            item("w-00000001", Done, 1, 50, None),
            item("w-00000002", Open, 10, 10, Some(30)),
            item("w-00000003", Open, 20, 20, None),
            item("w-00000004", Withdrawn, 2, 60, None),
            item("w-00000005", InProgress, 30, 30, None),
            item("w-00000006", Open, 5, 5, Some(25)),
        ]);
        let ids: Vec<&str> = listed.iter().map(|w| &w.id[9..]).collect();
        assert_eq!(ids, ["5", "3", "6", "2", "4", "1"]);
    }

    #[test]
    fn at_most_100_closed_items_are_listed() {
        let many = (0..150)
            .map(|n| item(&format!("w-{n:08x}"), WorkState::Done, n, n, None))
            .collect();
        let listed = order(many);
        assert_eq!(listed.len(), MAX_CLOSED);
        assert_eq!(listed[0].updated_at, 149);
    }
}
