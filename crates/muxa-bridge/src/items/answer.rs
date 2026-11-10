//! `update-inbox` on a Task's item (ADR 0033 §1): each answer does what the
//! Task's own request would, then closes the item.
mod prompt;
#[cfg(test)]
mod tests;

use std::sync::Arc;

use muxa_protocol::{
    ChatPrompt, ErrorBody, ErrorCode, InboxAction, InboxItem, ItemKind, ItemState, TaskStatus,
};
use muxa_tmux::PaneId;

use super::store::{ItemStore, TaskItems};
use super::{answers, shown};
use crate::assistant::corrections;
use crate::convert::error_body;
use crate::records::now;
use crate::routines::store::StateError;
use crate::task_records::{Context, reply, restart, set_status};

/// The most bytes a `reply`, `redo` or `change-course` text has.
pub const MAX_TEXT: usize = 4096;

pub struct Answer {
    pub item: String,
    pub action: InboxAction,
    pub text: Option<String>,
    pub option: Option<u32>,
    /// With an answer that ends the Task (ADR 0043).
    pub worktree: Option<muxa_protocol::WorktreeEnd>,
}

/// What an accepted answer does before the item closes.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Plan {
    Read,
    /// Sets or clears `protectedAt` (ADR 0046). Any item, open or closed.
    Protect(bool),
    /// Moves the item down (ADR 0040).
    Later,
    Status(TaskStatus),
    Restart,
    /// `branch-task` on a reading Task (ADR 0034 §3).
    Branch,
    /// The option at `option` of the menu the item asked about.
    Pick {
        pane: PaneId,
        asked: ChatPrompt,
        option: usize,
    },
    /// The next prompt for the agent. Never logged (invariant 5).
    Reply(String),
    /// An `ask`'s `reply`: the menu's cancel option at `option`, then `text`
    /// once the adapter reports the menu gone.
    DenyAndReply {
        pane: PaneId,
        asked: ChatPrompt,
        option: usize,
        text: String,
    },
    Nothing,
}

pub async fn update(cx: &Context, answer: Answer) -> Result<InboxItem, ErrorBody> {
    let (task, item) = find(cx, &answer.item).await?;
    let plan = plan(&shown(&item), &answer)?;
    let reason = reason(&item, &answer)?;
    act(cx, &task, plan.clone(), answer.worktree).await?;
    let saved = close(cx, &task, &answer.item, answer.action, plan).await?;
    tracing::info!(item = %answer.item, task, action = ?answer.action, "inbox item answered");
    let (config, item, action) = (Arc::clone(&cx.records), saved.clone(), answer.action);
    // The item is written; the correction is a signal on top (ADR 0050 §2).
    let recorded = blocking(move || {
        corrections::after_answer(&config, &item, action, reason, now());
        Ok(())
    })
    .await;
    if let Err(e) = recorded {
        tracing::warn!(item = %saved.id, error = %e.message, "cannot record a correction");
    }
    Ok(saved)
}

/// A correction's reason: `text` with `later`, and with `reject` on a
/// `sort` or `delegate` item (ADR 0050 §2).
fn reason(item: &InboxItem, answer: &Answer) -> Result<Option<String>, ErrorBody> {
    let proposal = matches!(
        item.kind,
        ItemKind::Sort | ItemKind::Delegate | ItemKind::Write
    );
    match answer.action {
        InboxAction::Later => corrections::check_reason(answer.text.as_deref()),
        InboxAction::Reject if proposal => corrections::check_reason(answer.text.as_deref()),
        _ => Ok(None),
    }
}

/// The Task that holds the item, and the item as stored.
async fn find(cx: &Context, id: &str) -> Result<(String, InboxItem), ErrorBody> {
    let (config, id) = (Arc::clone(&cx.records), id.to_owned());
    blocking(move || {
        let unknown = || bad(format!("no Inbox item {id}; list the Inbox again"));
        let Some(dir) = config.state_dir.as_ref().filter(|d| d.is_dir()) else {
            return Err(unknown());
        };
        let store = ItemStore::open(dir).map_err(|e| state_error(&e))?;
        let task = store
            .owner(&id)
            .map_err(|e| state_error(&e))?
            .ok_or_else(unknown)?;
        let items = store.get(&task).map_err(|e| state_error(&e))?;
        let item = items
            .items
            .into_iter()
            .find(|s| s.item.id == id)
            .ok_or_else(unknown)?;
        Ok((task, item.item))
    })
    .await
}

/// Pure. `item` is as shown.
fn plan(item: &InboxItem, answer: &Answer) -> Result<Plan, ErrorBody> {
    use InboxAction::*;
    // Accepted on any item and never listed in `answers`.
    let always = matches!(answer.action, Read | Protect | Unprotect);
    let allowed = answers(item);
    if !always && !allowed.contains(&answer.action) {
        return Err(not_allowed(item, answer.action, &allowed));
    }
    Ok(match answer.action {
        Later => Plan::Later,
        Accept => Plan::Status(TaskStatus::Done),
        Drop => Plan::Status(TaskStatus::Dropped),
        Retry => Plan::Restart,
        Branch => Plan::Branch,
        // `answers` never lists it, so the check above refused it already.
        Unknown => return Err(not_allowed(item, answer.action, &allowed)),
        // `routines::handle` sends them to `assistant::hypotheses` first.
        Confirm => return Err(bad("confirm on a hypothesis runs through the assistant")),
        Reject if item.kind == ItemKind::Hypothesis => {
            return Err(bad("reject on a hypothesis runs through the assistant"));
        }
        // Approve runs in `assistant::proposals`, which `routines::handle` calls first.
        // Reject on a hand-off ends the chain with the results it has (ADR 0051 §3);
        // on a write it makes no Task (ADR 0052 §2).
        Reject
            if matches!(
                item.kind,
                ItemKind::Sort | ItemKind::Delegate | ItemKind::Handoff | ItemKind::Write
            ) =>
        {
            Plan::Nothing
        }
        Approve
            if matches!(
                item.kind,
                ItemKind::Sort | ItemKind::Delegate | ItemKind::Handoff | ItemKind::Write
            ) =>
        {
            return Err(bad("approve on a proposal item runs through the assistant"));
        }
        Approve | Reject | Choose => pick(item, answer.action, answer.option)?,
        Reply => {
            let text = reply_text(Reply, answer.text.as_deref())?;
            if item.kind != ItemKind::Ask {
                return Ok(Plan::Reply(text));
            }
            match pick(item, Reject, None)? {
                Plan::Pick {
                    pane,
                    asked,
                    option,
                } => Plan::DenyAndReply {
                    pane,
                    asked,
                    option,
                    text,
                },
                other => other,
            }
        }
        Read => Plan::Read,
        Protect => Plan::Protect(true),
        Unprotect => Plan::Protect(false),
        KeepOpen | Keep | Dismiss => Plan::Nothing,
        // `routines::handle` sends it to `items::ask_agent` first.
        AskAgent => return Err(bad("ask-agent is not an answer to the Task")),
    })
}

fn not_allowed(item: &InboxItem, action: InboxAction, allowed: &[InboxAction]) -> ErrorBody {
    if allowed.is_empty() {
        return bad(format!("Inbox item {} is already closed", item.id));
    }
    let names: Vec<String> = allowed.iter().map(|a| name(*a)).collect();
    bad(format!(
        "{} does not answer Inbox item {} now; it takes {}, or read",
        name(action),
        item.id,
        names.join(", ")
    ))
}

/// The menu option an `ask`'s answer picks.
fn pick(item: &InboxItem, action: InboxAction, option: Option<u32>) -> Result<Plan, ErrorBody> {
    if item.kind != ItemKind::Ask {
        return Err(bad(format!(
            "{} answers a Routine's proposal through its entry id",
            name(action)
        )));
    }
    let asked = item
        .prompt
        .clone()
        .ok_or_else(|| bad("the agent's menu is unknown; answer it in the terminal"))?;
    let pane: PaneId = item
        .pane
        .as_deref()
        .ok_or_else(|| bad("the item has no pane; answer it in the terminal"))?
        .parse()
        .map_err(|e| crate::convert::error(&e))?;
    let index = match action {
        InboxAction::Approve => asked.options.iter().position(|o| !o.cancel),
        InboxAction::Reject => asked.options.iter().position(|o| o.cancel),
        _ => Some(choice(option, asked.options.len())?),
    };
    let option =
        index.ok_or_else(|| bad(format!("the menu has no option for {}", name(action))))?;
    Ok(Plan::Pick {
        pane,
        asked,
        option,
    })
}

fn choice(option: Option<u32>, options: usize) -> Result<usize, ErrorBody> {
    let n = option.ok_or_else(|| bad("choose needs option, an index into the prompt's options"))?;
    usize::try_from(n)
        .ok()
        .filter(|&i| i < options)
        .ok_or_else(|| {
            bad(format!(
                "option {n} is not one of the prompt's {options} options"
            ))
        })
}

/// The prompt the agent gets: the text as it is.
fn reply_text(action: InboxAction, text: Option<&str>) -> Result<String, ErrorBody> {
    let text = text.unwrap_or_default();
    if text.is_empty() || text.len() > MAX_TEXT {
        return Err(bad(format!(
            "{} needs a text of 1 to {MAX_TEXT} bytes",
            name(action)
        )));
    }
    Ok(text.to_owned())
}

async fn act(
    cx: &Context,
    task: &str,
    plan: Plan,
    worktree: Option<muxa_protocol::WorktreeEnd>,
) -> Result<(), ErrorBody> {
    match plan {
        Plan::Status(status) => {
            let ends = matches!(status, TaskStatus::Done | TaskStatus::Dropped);
            if let Some(choice) = worktree.filter(|_| ends) {
                return crate::task_records::end::end(cx, task, None, status, choice)
                    .await
                    .map(drop);
            }
            set_status(&cx.records, task.to_owned(), status).await?;
            crate::task_records::close::close_when_ended(cx, task, status);
            Ok(())
        }
        Plan::Restart => {
            let env = cx.env().await?;
            restart(&env, task.to_owned(), false, None, None)
                .await
                .map(drop)
        }
        Plan::Branch => {
            let env = cx.env().await?;
            crate::task_records::branch_task::branch(&env, task.to_owned(), None)
                .await
                .map(drop)
        }
        Plan::Pick {
            pane,
            asked,
            option,
        } => prompt::pick(cx, pane, &asked, option).await,
        Plan::Reply(text) => reply::send(&cx.env().await?, task, &text).await,
        Plan::DenyAndReply {
            pane,
            asked,
            option,
            text,
        } => {
            prompt::pick(cx, pane, &asked, option).await?;
            prompt::wait_taken(cx, pane, &asked).await?;
            reply::send_after_deny(&cx.env().await?, task, &text).await
        }
        Plan::Read | Plan::Protect(_) | Plan::Later | Plan::Nothing => Ok(()),
    }
}

/// Writes the answer under the store's lock and returns the item as shown.
async fn close(
    cx: &Context,
    task: &str,
    id: &str,
    action: InboxAction,
    plan: Plan,
) -> Result<InboxItem, ErrorBody> {
    let (config, task, id) = (Arc::clone(&cx.records), task.to_owned(), id.to_owned());
    blocking(move || {
        let dir = config
            .state_dir
            .as_ref()
            .ok_or_else(|| error_body(ErrorCode::Internal, "the state directory went away"))?;
        let store = ItemStore::open(dir).map_err(|e| state_error(&e))?;
        store
            .update(&task, |items| apply(items, &id, action, &plan, now()))
            .map_err(|e| match e {
                Failed::State(e) => state_error(&e),
                Failed::Body(e) => e,
            })
    })
    .await
}

fn apply(
    items: &mut TaskItems,
    id: &str,
    action: InboxAction,
    plan: &Plan,
    at: u64,
) -> Result<InboxItem, Failed> {
    let stored = items
        .items
        .iter_mut()
        .find(|s| s.item.id == id)
        .ok_or_else(|| Failed::Body(bad(format!("Inbox item {id} was removed"))))?;
    let item = &mut stored.item;
    match plan {
        Plan::Read => {
            item.read_at.get_or_insert(at);
        }
        Plan::Protect(true) => {
            item.protected_at.get_or_insert(at);
        }
        Plan::Protect(false) => item.protected_at = None,
        Plan::Later => {
            item.state = ItemState::Open;
            item.later_at = Some(at);
            item.until = None;
        }
        // A second device's answer came first: it stays. A `resolved` one
        // may be this answer's own effect, so the answer wins.
        _ if item.state == ItemState::Answered => {}
        _ => {
            item.state = ItemState::Answered;
            item.answer = Some(action);
            item.answered_at = Some(at);
            item.until = None;
        }
    }
    item.answers = answers(item);
    Ok(shown(item))
}

enum Failed {
    State(StateError),
    Body(ErrorBody),
}

impl From<StateError> for Failed {
    fn from(e: StateError) -> Self {
        Self::State(e)
    }
}

/// The action's wire name, e.g. `change-course`.
fn name(action: InboxAction) -> String {
    serde_json::to_value(action)
        .ok()
        .and_then(|v| v.as_str().map(str::to_owned))
        .unwrap_or_else(|| format!("{action:?}"))
}

fn bad(message: impl Into<String>) -> ErrorBody {
    error_body(ErrorCode::BadRequest, message)
}

fn state_error(e: &StateError) -> ErrorBody {
    match e {
        StateError::Lock(crate::records::StoreError::Busy(_)) => error_body(
            ErrorCode::Conflict,
            "the Inbox item is being written; try again",
        ),
        e => {
            tracing::warn!(error = %e, "inbox item store failed");
            error_body(ErrorCode::Internal, e.to_string())
        }
    }
}

async fn blocking<T: Send + 'static>(
    f: impl FnOnce() -> Result<T, ErrorBody> + Send + 'static,
) -> Result<T, ErrorBody> {
    tokio::task::spawn_blocking(f)
        .await
        .unwrap_or_else(|_| Err(error_body(ErrorCode::Internal, "the item store stopped")))
}
