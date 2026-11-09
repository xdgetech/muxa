//! `in-progress` and `waiting-decision` come from a Work item's Tasks and
//! requests each time it is listed (ADR 0046 §2). They are never stored.
use std::collections::HashMap;

use muxa_protocol::{AssistantRequest, Delivery, ItemState, WorkItem, WorkState};

use crate::items::store::ItemStore;
use crate::records::Status;
use crate::task_records::Config;

/// Blocking. Sets the derived state of each open item.
pub fn states(config: &Config, items: &mut [WorkItem], requests: &[AssistantRequest]) {
    if items.iter().all(|w| super::work::closed(w.state)) {
        return;
    }
    let tasks: HashMap<String, Status> = config
        .records_if_any()
        .into_iter()
        .map(|r| (r.id, r.status))
        .collect();
    let store = config
        .state_dir
        .as_ref()
        .and_then(|d| ItemStore::open(d).ok());
    let waiting = |task: &str| {
        store.as_ref().is_some_and(|s| {
            s.get(task).is_ok_and(|t| {
                t.items
                    .iter()
                    .any(|i| matches!(i.item.state, ItemState::Open | ItemState::Later))
            })
        })
    };
    let running = |request: &str| {
        requests.iter().any(|r| {
            r.id == request
                && matches!(
                    r.delivery,
                    Delivery::Pending
                        | Delivery::Received
                        | Delivery::Processing
                        | Delivery::Unconfirmed
                        | Delivery::CancelRequested
                )
        })
    };
    for item in items.iter_mut().filter(|w| !super::work::closed(w.state)) {
        let live: Vec<&String> = item
            .tasks
            .iter()
            .filter(|t| matches!(tasks.get(*t), Some(Status::Open | Status::Review)))
            .collect();
        item.state = if live.iter().any(|t| waiting(t)) {
            WorkState::WaitingDecision
        } else if item
            .tasks
            .iter()
            .any(|t| tasks.get(t) == Some(&Status::Open))
            || item.requests.iter().any(|r| running(r))
        {
            WorkState::InProgress
        } else {
            WorkState::Open
        };
    }
}
