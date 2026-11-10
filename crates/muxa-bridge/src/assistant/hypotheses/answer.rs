//! `confirm` and `reject` on a `hypothesis` item (ADR 0048 §3). Confirm
//! makes one policy, then retires the hypothesis, then closes the item, so
//! a retry after a crash finds the policy and finishes. Reject keeps the
//! hypothesis `rejected` for 90 days and records a `rejected` correction.
use std::sync::Arc;

use muxa_protocol::{
    ErrorBody, ErrorCode, Hypothesis, HypothesisState, InboxAction, InboxItem, ItemKind, ItemState,
    Message, Policy,
};

use super::REJECTED_FOR;
use crate::assistant::{corrections, policies, state_error};
use crate::convert::error_body;
use crate::items::store::ItemStore;
use crate::routines::store::StateError;
use crate::task_records::{Config, Context};

/// `update-inbox confirm` or `reject` on a `hypothesis` item: its messages
/// and the error for `result`. None for any other item.
pub async fn answer(
    cx: &Context,
    id: u64,
    entry: &str,
    action: InboxAction,
    text: Option<String>,
) -> Option<(Vec<Message>, Option<ErrorBody>)> {
    let (config, entry) = (Arc::clone(&cx.records), entry.to_owned());
    let done = tokio::task::spawn_blocking(move || {
        let now = crate::records::now();
        let Some((task, item)) = find(&config, &entry)? else {
            return Ok(None);
        };
        match action {
            InboxAction::Confirm => confirm(&config, &task, &item, now).map(|(policy, item)| {
                Some(vec![
                    Message::PolicySaved {
                        policy: id,
                        entry: policy,
                    },
                    saved(id, item),
                ])
            }),
            InboxAction::Reject => {
                let reason = corrections::check_reason(text.as_deref())?;
                reject(&config, &task, &item, reason, now).map(|item| Some(vec![saved(id, item)]))
            }
            _ => Ok(None),
        }
    })
    .await
    .unwrap_or_else(|_| {
        Err(error_body(
            ErrorCode::Internal,
            "the assistant store stopped",
        ))
    });
    match done {
        Ok(Some(messages)) => Some((messages, None)),
        Ok(None) => None,
        Err(e) => Some((Vec::new(), Some(e))),
    }
}

fn saved(id: u64, item: InboxItem) -> Message {
    Message::InboxSaved {
        inbox: id,
        entry: None,
        item: Some(Box::new(item)),
    }
}

/// Blocking. The Task file and the item, when `entry` is a `hypothesis` item.
pub fn find(config: &Config, entry: &str) -> Result<Option<(String, InboxItem)>, ErrorBody> {
    let Some(dir) = config.state_dir.as_ref().filter(|d| d.is_dir()) else {
        return Ok(None);
    };
    let store = ItemStore::open(dir).map_err(|e| state_error(&e))?;
    let Some(task) = store.owner(entry).map_err(|e| state_error(&e))? else {
        return Ok(None);
    };
    let items = store.get(&task).map_err(|e| state_error(&e))?;
    Ok(items
        .items
        .into_iter()
        .map(|s| s.item)
        .find(|i| i.id == entry && i.kind == ItemKind::Hypothesis)
        .map(|i| (task, i)))
}

/// Blocking. The policy the hypothesis became, and the item as shown.
pub fn confirm(
    config: &Config,
    task: &str,
    item: &InboxItem,
    now: u64,
) -> Result<(Policy, InboxItem), ErrorBody> {
    let h = hypothesis_of(config, item)?;
    let policy = policies::from_hypothesis(config, &h, now)?;
    set_state(config, &h.id, |h| {
        if h.state == HypothesisState::Proposed {
            h.state = HypothesisState::Confirmed;
        }
    })?;
    let (item, _) = close(config, task, &item.id, InboxAction::Confirm, now)?;
    tracing::info!(item = %item.id, hypothesis = %h.id, policy = %policy.id, "hypothesis confirmed");
    Ok((policy, item))
}

/// Blocking. The item as shown. A second reject changes nothing.
pub fn reject(
    config: &Config,
    task: &str,
    item: &InboxItem,
    reason: Option<String>,
    now: u64,
) -> Result<InboxItem, ErrorBody> {
    let h = hypothesis_of(config, item)?;
    if h.state == HypothesisState::Confirmed {
        let made = policies::list(config, now)?
            .into_iter()
            .find(|p| p.source == h.id)
            .map_or_else(|| "a policy".to_owned(), |p| p.id);
        return Err(bad(format!(
            "hypothesis {} became {made}: delete the policy with update-policy",
            h.id
        )));
    }
    set_state(config, &h.id, |h| {
        if h.state != HypothesisState::Rejected {
            h.state = HypothesisState::Rejected;
            h.expires_at = now + REJECTED_FOR;
        }
    })?;
    let (shown, changed) = close(config, task, &item.id, InboxAction::Reject, now)?;
    if changed {
        corrections::after_answer(config, &shown, InboxAction::Reject, reason, now);
    }
    tracing::info!(item = %shown.id, hypothesis = %h.id, "hypothesis rejected");
    Ok(shown)
}

fn hypothesis_of(config: &Config, item: &InboxItem) -> Result<Hypothesis, ErrorBody> {
    let id = item
        .hypothesis
        .as_ref()
        .map(|h| h.hypothesis.clone())
        .ok_or_else(|| bad(format!("Inbox item {} names no hypothesis", item.id)))?;
    crate::assistant::shelf::<Hypothesis>(config)?
        .get(&id)
        .map_err(|e| state_error(&e))?
        .ok_or_else(|| bad(format!("hypothesis {id} was forgotten")))
}

fn set_state(
    config: &Config,
    id: &str,
    change: impl FnOnce(&mut Hypothesis),
) -> Result<(), ErrorBody> {
    crate::assistant::shelf::<Hypothesis>(config)?
        .update::<_, StateError>(id, |h| {
            change(h);
            Ok(())
        })
        .map_err(|e| state_error(&e))?
        .ok_or_else(|| bad(format!("hypothesis {id} was forgotten")))
}

/// Answers the item when it is open, under its file's lock. A second
/// device's answer stays. The item as shown, and whether it changed.
fn close(
    config: &Config,
    task: &str,
    id: &str,
    action: InboxAction,
    at: u64,
) -> Result<(InboxItem, bool), ErrorBody> {
    let dir = config
        .state_dir
        .as_ref()
        .ok_or_else(|| error_body(ErrorCode::Internal, "the state directory went away"))?;
    let store = ItemStore::open(dir).map_err(|e| state_error(&e))?;
    let closed = store
        .update::<_, StateError>(task, |items| {
            let Some(s) = items.items.iter_mut().find(|s| s.item.id == id) else {
                return Ok(None);
            };
            let item = &mut s.item;
            let open = matches!(item.state, ItemState::Open | ItemState::Later);
            if open {
                item.state = ItemState::Answered;
                item.answer = Some(action);
                item.answered_at = Some(at);
                item.until = None;
            }
            item.answers = crate::items::answers(item);
            Ok(Some((crate::items::shown(item), open)))
        })
        .map_err(|e| state_error(&e))?;
    closed.ok_or_else(|| bad(format!("Inbox item {id} was removed")))
}

fn bad(message: impl Into<String>) -> ErrorBody {
    error_body(ErrorCode::BadRequest, message.into())
}

#[cfg(test)]
#[path = "answer_tests.rs"]
pub(crate) mod tests;
