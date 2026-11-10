//! Captures, Work items and assistant requests (ADR 0046): the user's input
//! and unfinished work, kept apart from Tasks and Inbox items. Plans
//! (ADR 0047), the user's policies and corrections (ADR 0048) live here too.
pub mod apply;
pub mod ask;
mod assist;
pub mod brief;
mod cancel;
pub mod capture;
pub mod check;
mod cleanup;
pub mod corrections;
pub mod delivery;
mod derive;
pub mod detail;
pub mod info;
pub mod memory;
pub mod notes;
pub mod overall_brief;
pub mod plan;
pub mod policies;
pub mod policy;
pub mod proposals;
pub mod record;
pub mod result;
pub mod share;
pub mod start;
pub mod store;
mod tick;
pub mod work;

pub use assist::NO_DAEMON;
pub use tick::tick;

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

/// Answers a `work` level 8, 9 or 10 request: its saved message, and its error.
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
            let spec = assist.clone();
            let new = capture::NewCapture {
                client_id,
                text,
                source,
                workspace,
                assist,
            };
            let entry = match blocking(move || capture::save(&config, new)).await {
                Ok((entry, _)) => entry,
                Err(e) => return (Vec::new(), Some(e)),
            };
            // The input is kept even when the start is refused (ADR 0046 §8).
            let done = match spec {
                Some(spec) => assist::assist(cx, entry, spec).await,
                None => assist::Assisted {
                    capture: entry,
                    work: None,
                    request: None,
                    error: None,
                },
            };
            let saved = Message::CaptureSaved {
                capture: id,
                entry: done.capture,
                work: done.work.map(Box::new),
                request: done.request.map(Box::new),
            };
            (vec![saved], done.error)
        }
        Request::SortCapture {
            id,
            capture,
            rev,
            action,
            work,
            title,
            workspace,
        } => {
            let config = Arc::clone(&cx.records);
            let sort = work::Sort {
                capture,
                rev,
                action,
                work,
                title,
                workspace,
            };
            let sorted = blocking(move || {
                let (entry, item) = work::sort(&config, sort)?;
                Ok((entry, item.map(|i| listed(&config, i))))
            })
            .await;
            match sorted {
                Ok((entry, item)) => (
                    vec![Message::CaptureSaved {
                        capture: id,
                        entry,
                        work: item.map(Box::new),
                        request: None,
                    }],
                    None,
                ),
                Err(e) => (Vec::new(), Some(e)),
            }
        }
        Request::UpdateWork {
            id,
            work,
            rev,
            action,
            title,
            workspace,
            into,
            due,
        } => {
            let config = Arc::clone(&cx.records);
            let update = work::Update {
                work,
                rev,
                action,
                title,
                workspace,
                into,
                due,
            };
            let saved = blocking(move || {
                let item = work::update(&config, update)?;
                let changed = item.rev != rev;
                corrections::after_work(&config, &item, action, changed, crate::records::now());
                Ok(listed(&config, item))
            })
            .await;
            match saved {
                Ok(entry) => (
                    vec![Message::WorkSaved {
                        work: id,
                        entry,
                        request: None,
                    }],
                    None,
                ),
                Err(e) => (Vec::new(), Some(e)),
            }
        }
        Request::RequestAssistant {
            id,
            work,
            rev,
            scope,
            workspace,
            purpose,
        } => {
            if !cx.daemon {
                return (
                    Vec::new(),
                    Some(error_body(ErrorCode::Unsupported, NO_DAEMON)),
                );
            }
            let a = ask::Ask {
                work,
                rev,
                scope,
                workspace,
                purpose,
            };
            match ask::ask(cx, a).await {
                Ok((entry, request, error)) => (
                    vec![Message::WorkSaved {
                        work: id,
                        entry,
                        request: Some(Box::new(request)),
                    }],
                    error,
                ),
                Err(e) => (Vec::new(), Some(e)),
            }
        }
        Request::CancelRequest {
            id,
            request,
            version,
        } => {
            if !cx.daemon {
                return (
                    Vec::new(),
                    Some(error_body(ErrorCode::Unsupported, NO_DAEMON)),
                );
            }
            match cancel::cancel(cx, request, version).await {
                Ok((entry, request)) => (
                    vec![Message::WorkSaved {
                        work: id,
                        entry,
                        request: Some(Box::new(request)),
                    }],
                    None,
                ),
                Err(e) => (Vec::new(), Some(e)),
            }
        }
        Request::GetWork { id, work } => {
            let config = Arc::clone(&cx.records);
            match blocking(move || detail::detail(&config, &work)).await {
                Ok(d) => (
                    vec![Message::WorkDetail {
                        work: id,
                        entry: d.entry,
                        captures: d.captures,
                        requests: d.requests,
                    }],
                    None,
                ),
                Err(e) => (Vec::new(), Some(e)),
            }
        }
        Request::ApplyNotes {
            id,
            source,
            version,
            base,
        } => {
            let config = Arc::clone(&cx.records);
            match blocking(move || apply::apply_notes(&config, &source, version, &base)).await {
                Ok(apply::Applied::Request(entry, request)) => (
                    vec![Message::WorkSaved {
                        work: id,
                        entry,
                        request: Some(Box::new(request)),
                    }],
                    None,
                ),
                Ok(apply::Applied::Plan(entry)) => (
                    vec![Message::PlanSaved {
                        plan: id,
                        entry: Box::new(entry),
                    }],
                    None,
                ),
                Err(e) => (Vec::new(), Some(e)),
            }
        }
        Request::PlanNow { id, minutes } => {
            if !cx.daemon {
                return (
                    Vec::new(),
                    Some(error_body(ErrorCode::Unsupported, NO_DAEMON)),
                );
            }
            plan::now(cx, id, minutes).await
        }
        Request::AddPolicy {
            id,
            text,
            scope,
            from,
            until,
        } => {
            let config = Arc::clone(&cx.records);
            let fields = policies::Fields {
                text: Some(text),
                scope: Some(scope),
                from,
                until,
            };
            let now = crate::records::now();
            match blocking(move || policies::add(&config, fields, now)).await {
                Ok(entry) => (vec![Message::PolicySaved { policy: id, entry }], None),
                Err(e) => (Vec::new(), Some(e)),
            }
        }
        Request::UpdatePolicy {
            id,
            policy,
            rev,
            action,
            text,
            scope,
            from,
            until,
        } => {
            let config = Arc::clone(&cx.records);
            let change = policies::Change {
                policy,
                rev,
                action,
                fields: policies::Fields {
                    text,
                    scope,
                    from,
                    until,
                },
            };
            let now = crate::records::now();
            match blocking(move || policies::update(&config, change, now)).await {
                Ok(saved) => (
                    saved
                        .map(|entry| Message::PolicySaved { policy: id, entry })
                        .into_iter()
                        .collect(),
                    None,
                ),
                Err(e) => (Vec::new(), Some(e)),
            }
        }
        Request::UpdatePlan {
            id,
            plan,
            rev,
            action,
            order,
            row,
            text,
        } => {
            let config = Arc::clone(&cx.records);
            let change = plan::update::Change {
                plan,
                rev,
                action,
                order,
                row,
                text,
            };
            let now = crate::records::now();
            let saved = blocking(move || {
                let entry = plan::update::update(&config, change, now)?;
                Ok(plan::listed(&config, entry))
            })
            .await;
            match saved {
                Ok(entry) => (
                    vec![Message::PlanSaved {
                        plan: id,
                        entry: Box::new(entry),
                    }],
                    None,
                ),
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

/// Blocking. The item as a list shows it, with its derived state.
fn listed(config: &Config, item: WorkItem) -> WorkItem {
    let requests = detail::requests_of(config, &item).unwrap_or_default();
    let mut one = [item];
    derive::states(config, &mut one, &requests);
    let [item] = one;
    item
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
    let mut work = order(
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
    let mut requests = shelf::<AssistantRequest>(config)?
        .all()
        .map_err(|e| state_error(&e))?
        .into_iter()
        .filter(|r| wanted.contains(r.id.as_str()))
        .collect::<Vec<_>>();
    delivery::refresh(config, &mut requests, crate::records::now());
    derive::states(config, &mut work, &requests);
    Ok(Listing {
        work,
        captures,
        requests,
    })
}

/// Pinned items, in the order pinned (ADR 0047 §6); then open items, newest
/// `createdAt` first; then the ones moved down with Later, in the order
/// moved; then at most [`MAX_CLOSED`] closed ones, newest `updatedAt` first.
fn order(items: Vec<WorkItem>) -> Vec<WorkItem> {
    let closed = |w: &WorkItem| matches!(w.state, WorkState::Done | WorkState::Withdrawn);
    let (mut done, rest): (Vec<_>, Vec<_>) = items.into_iter().partition(closed);
    let (mut pinned, rest): (Vec<_>, Vec<_>) =
        rest.into_iter().partition(|w| w.pinned_at.is_some());
    let (mut later, mut open): (Vec<_>, Vec<_>) =
        rest.into_iter().partition(|w| w.later_at.is_some());
    pinned.sort_by(|a, b| a.pinned_at.cmp(&b.pinned_at).then(a.id.cmp(&b.id)));
    open.sort_by(|a, b| b.created_at.cmp(&a.created_at).then(b.id.cmp(&a.id)));
    later.sort_by(|a, b| a.later_at.cmp(&b.later_at).then(a.id.cmp(&b.id)));
    done.sort_by(|a, b| b.updated_at.cmp(&a.updated_at).then(b.id.cmp(&a.id)));
    done.truncate(MAX_CLOSED);
    pinned
        .into_iter()
        .chain(open)
        .chain(later)
        .chain(done)
        .collect()
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
            pinned_at: None,
            due: None,
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
    fn pinned_items_come_first_in_the_order_pinned() {
        use WorkState::*;
        let pinned = |id: &str, created: u64, at: u64| WorkItem {
            pinned_at: Some(at),
            ..item(id, Open, created, created, None)
        };
        let listed = order(vec![
            item("w-00000001", Open, 50, 50, None),
            pinned("w-00000002", 10, 300),
            item("w-00000003", Open, 5, 5, Some(25)),
            pinned("w-00000004", 40, 200),
            item("w-00000005", Done, 1, 60, None),
        ]);
        let ids: Vec<&str> = listed.iter().map(|w| &w.id[9..]).collect();
        assert_eq!(ids, ["4", "2", "1", "3", "5"]);
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
