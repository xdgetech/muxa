//! What follows a step's Result (ADR 0051 §3, ADR 0058 §1). The daemon's
//! tick makes one `result` Inbox item in the step's run file, which offers
//! the step's hand-off; under `handoffs = "auto"` it starts the next step
//! instead, within every budget and `max_auto_handoffs_per_day`, else makes
//! the item with the limit. A new version resolves the item; a cancel ends
//! the chain and takes the hand-off off it. Logs carry ids, steps and
//! worker names, never a purpose or a Result.
pub mod items;
pub mod next;

use std::sync::Arc;

use muxa_protocol::{AssistResult, AssistantRequest, Delivery, Handoff};
use serde::{Deserialize, Serialize};

use super::store::{Extra, Shelf};
use crate::records::Workspaces;
use crate::records::staff::Handoffs;
use crate::routines::store::StateError;
use crate::task_records::{Config, Context};

/// The request file's mark of the newest step's item, or of an ended chain.
/// Its name is from when only a hand-off made an item.
pub const WAITING_FIELD: &str = "handoffWaiting";
/// Seconds a Result stays new enough to make an item: an upgraded host does
/// not ask about old ones (ADR 0058 §1).
pub const RECENT: u64 = 86_400;

/// The item made for step `step`'s Result, and whether its chain ended.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Waiting {
    pub version: u64,
    pub step: u32,
    /// The run whose item file holds the item. None: no item yet.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task: Option<String>,
    /// A cancel ended the chain before its next step.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub ended: bool,
    /// The item was closed because the chain ended.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub resolved: bool,
}

pub fn waiting(extra: &Extra) -> Option<Waiting> {
    serde_json::from_value(extra.get(WAITING_FIELD)?.clone()).ok()
}

pub fn set_waiting(extra: &mut Extra, w: &Waiting) {
    match serde_json::to_value(w) {
        Ok(v) => {
            extra.insert(WAITING_FIELD.into(), v);
        }
        Err(e) => tracing::warn!(error = %e, "cannot keep a chain's mark"),
    }
}

/// The current version's newest step once its Result is stored: the step,
/// the hand-off it proposes, and the Result. None while it runs, or when it
/// failed or was cancelled. A request without staff is step 1.
pub fn newest(q: &AssistantRequest) -> Option<(u32, Option<&Handoff>, &AssistResult)> {
    let step = q.steps.last().map_or(1, |s| s.step);
    let result = q
        .results
        .iter()
        .find(|r| r.version == q.version && r.step.unwrap_or(1) == step)?;
    if q.delivery != Delivery::Resulted {
        return None;
    }
    Some((
        step,
        q.steps.last().and_then(|s| s.handoff.as_ref()),
        result,
    ))
}

/// The step whose stored result asks for a hand-off that has not started
/// yet, with the hand-off. None for a request without staff, or a version
/// whose newest step is running, failed or cancelled.
pub fn pending(q: &AssistantRequest) -> Option<(u32, &Handoff)> {
    let (step, handoff, _) = newest(q)?;
    Some((step, handoff?))
}

/// Whether the chain of `version` ended without its next step.
pub fn ended(extra: &Extra, version: u64) -> bool {
    waiting(extra).is_some_and(|w| w.ended && w.version == version)
}

/// What the tick does for one request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Todo {
    /// The `result` item; `handoff` when it offers the step's hand-off.
    Ask { step: u32, handoff: bool },
    /// `handoffs = "auto"`: the next step.
    Start { step: u32 },
    /// The item no longer applies: a newer version replaced its Result.
    Resolve { task: String },
    /// A cancel ended the chain: the item's hand-off can no longer start.
    End { task: String },
}

/// Why [`Todo::Resolve`] closes an item.
pub const REPLACED: &str = "A newer version of this request replaced this result.";
/// Why [`Todo::End`] takes the hand-off off an item.
pub const CANCELLED: &str = "The request was cancelled; its chain ended.";

/// Pure. What one request's chain needs now, at `now`.
pub(super) fn todo(
    q: &AssistantRequest,
    extra: &Extra,
    mode: Option<Handoffs>,
    now: u64,
) -> Option<Todo> {
    let mark = waiting(extra);
    if let Some(w) = &mark {
        if let (Some(task), false) = (&w.task, w.resolved) {
            if w.version != q.version {
                return Some(Todo::Resolve { task: task.clone() });
            }
            if matches!(q.delivery, Delivery::CancelRequested | Delivery::Cancelled) || w.ended {
                return Some(Todo::End { task: task.clone() });
            }
        }
    }
    let (step, handoff, result) = newest(q)?;
    if mark.is_some_and(|w| w.version == q.version && w.step == step) {
        return None;
    }
    match (handoff, mode) {
        (Some(_), Some(Handoffs::Auto)) => Some(Todo::Start { step }),
        _ if now.saturating_sub(result.at) > RECENT => None,
        // `none` refuses a hand-off in the check; configuration changed since.
        (Some(_), Some(Handoffs::None)) => Some(Todo::Ask {
            step,
            handoff: false,
        }),
        (handoff, _) => Some(Todo::Ask {
            step,
            handoff: handoff.is_some(),
        }),
    }
}

/// The daemon's tick: items and resolutions in the store, then the `auto`
/// steps, one at a time.
pub async fn tick(cx: &Context) {
    let config = Arc::clone(&cx.records);
    let starts = tokio::task::spawn_blocking(move || scan(&config))
        .await
        .unwrap_or_else(|e| {
            tracing::warn!(error = %e, "the hand-off scan stopped");
            Vec::new()
        });
    for (request, version, step) in starts {
        auto(cx, request, version, step).await;
    }
}

/// Blocking. Makes and resolves items; returns the `auto` steps to start.
fn scan(config: &Config) -> Vec<(String, u64, u32)> {
    let Some(dir) = config.state_dir.as_ref().filter(|d| d.is_dir()) else {
        return Vec::new();
    };
    let opened = Shelf::<AssistantRequest>::open(dir).and_then(|s| Ok((s.all()?, s)));
    let (all, shelf) = match opened {
        Ok(found) => found,
        Err(e) => {
            tracing::warn!(error = %e, "cannot read requests for their hand-offs");
            return Vec::new();
        }
    };
    let workspaces = config.load_workspaces().unwrap_or_default();
    let now = crate::records::now();
    let mut starts = Vec::new();
    for q in all {
        let Ok(Some((q, extra))) = shelf.get_with_extra(&q.id) else {
            continue;
        };
        match todo(&q, &extra, mode(&workspaces, &q.workspace), now) {
            Some(Todo::Ask { step, handoff }) => ask(config, &shelf, &q, step, handoff, None),
            Some(Todo::Start { step }) => starts.push((q.id.clone(), q.version, step)),
            Some(Todo::Resolve { task }) => resolve(config, &shelf, &q, &task, false),
            Some(Todo::End { task }) => resolve(config, &shelf, &q, &task, true),
            None => {}
        }
    }
    starts
}

fn mode(workspaces: &Workspaces, workspace: &str) -> Option<Handoffs> {
    let d = workspaces.get(workspace)?.assistant.as_ref()?;
    Some(d.staff.handoffs)
}

/// Blocking. The `result` item of `step`, once, then the mark that it is
/// asked. `held` says why the host did not start the hand-off itself.
pub(super) fn ask(
    config: &Config,
    shelf: &Shelf<AssistantRequest>,
    q: &AssistantRequest,
    step: u32,
    handoff: bool,
    held: Option<String>,
) {
    let run = match q.steps.iter().find(|s| s.step == step) {
        Some(s) => s.run.clone(),
        None if q.steps.is_empty() => q.run.clone(),
        None => None,
    };
    let Some(task) = run else {
        tracing::warn!(request = %q.id, version = q.version, step, "a result's step has no run");
        return;
    };
    match items::make(config, &task, q, step, handoff, held) {
        Ok(Some(item)) => {
            tracing::info!(request = %q.id, version = q.version, step, item, handoff, "result item made")
        }
        Ok(None) => {}
        Err(e) => {
            tracing::warn!(request = %q.id, step, error = %e.message, "cannot make a result item");
            return;
        }
    }
    mark(shelf, &q.id, |extra| {
        set_waiting(
            extra,
            &Waiting {
                version: q.version,
                step,
                task: Some(task.clone()),
                ..Waiting::default()
            },
        );
    });
}

/// Blocking. Closes the item, or with `ended` takes its hand-off off it;
/// then marks it done with.
fn resolve(
    config: &Config,
    shelf: &Shelf<AssistantRequest>,
    q: &AssistantRequest,
    task: &str,
    ended: bool,
) {
    let closed = if ended {
        items::end(config, task, CANCELLED)
    } else {
        items::resolve(config, task, REPLACED)
    };
    if let Err(e) = closed {
        tracing::warn!(request = %q.id, task, error = %e.message, "cannot close a result item");
        return;
    }
    tracing::info!(request = %q.id, version = q.version, task, ended, "result item closed");
    mark(shelf, &q.id, |extra| {
        if let Some(mut w) = waiting(extra) {
            w.resolved = true;
            set_waiting(extra, &w);
        }
    });
}

fn mark(shelf: &Shelf<AssistantRequest>, id: &str, change: impl FnOnce(&mut Extra)) {
    let marked = shelf.update_with_extra::<_, StateError>(id, |_, extra| {
        change(extra);
        Ok(())
    });
    if let Err(e) = marked {
        tracing::warn!(request = %id, error = %e, "cannot mark a request's chain");
    }
}

/// Starts an `auto` step. Past a limit, or when it cannot start, the step
/// waits for review with the reason, and the host does not try it again.
async fn auto(cx: &Context, request: String, version: u64, step: u32) {
    let started = next::start(cx, request.clone(), version, step, next::Via::Auto).await;
    let why = match started {
        Ok((_, q, None)) => {
            tracing::info!(request, version, step = step + 1, worker = ?q.steps.last().map(|s| &s.worker), "hand-off started without review");
            return;
        }
        // The step was made and failed to start: the request shows it.
        Ok((_, _, Some(e))) => {
            tracing::info!(request, version, code = ?e.code, "an auto hand-off did not start");
            return;
        }
        Err(e) => e.message,
    };
    tracing::info!(request, version, step, "auto hand-off held for review");
    let config = Arc::clone(&cx.records);
    let held = tokio::task::spawn_blocking(move || {
        let shelf = super::shelf::<AssistantRequest>(&config)?;
        let Some(q) = shelf.get(&request).map_err(|e| super::state_error(&e))? else {
            return Ok(());
        };
        let held = format!("Not started automatically: {why} Hand it off to start it.");
        if q.version == version && pending(&q).is_some_and(|(s, _)| s == step) {
            ask(&config, &shelf, &q, step, true, Some(held));
        }
        Ok::<_, muxa_protocol::ErrorBody>(())
    })
    .await;
    match held {
        Ok(Ok(())) => {}
        Ok(Err(e)) => tracing::warn!(error = %e.message, "cannot hold a hand-off for review"),
        Err(e) => tracing::warn!(error = %e, "holding a hand-off for review stopped"),
    }
}

/// Blocking. Ends a chain that waits on a hand-off of `version`, for
/// `cancel-request`: the next step never starts, and its item closes. The
/// request's Work item when it ended one; None when nothing waits.
pub fn end(
    config: &Config,
    id: &str,
    version: u64,
) -> Result<Option<String>, muxa_protocol::ErrorBody> {
    let shelf = super::shelf::<AssistantRequest>(config)?;
    let ended = shelf
        .update_with_extra::<_, StateError>(id, |q, extra| {
            let Some((step, _)) = pending(q).filter(|_| q.version == version) else {
                return Ok(None);
            };
            let mut w = waiting(extra)
                .filter(|w| w.version == version && w.step == step)
                .unwrap_or(Waiting {
                    version,
                    step,
                    ..Waiting::default()
                });
            w.ended = true;
            set_waiting(extra, &w);
            Ok(Some((q.clone(), w)))
        })
        .map_err(|e| super::state_error(&e))?
        .flatten();
    let Some((q, w)) = ended else {
        return Ok(None);
    };
    tracing::info!(
        request = id,
        version,
        step = w.step,
        "chain ended by a cancel"
    );
    if let (Some(task), false) = (&w.task, w.resolved) {
        resolve(config, &shelf, &q, task, true);
    }
    Ok(Some(q.work))
}

/// Blocking. Done on a `result` item (ADR 0058 §2): the chain of `version`
/// ends there, and the tick leaves the answered item as it is.
pub fn done(config: &Config, id: &str, version: u64) -> Result<(), muxa_protocol::ErrorBody> {
    super::shelf::<AssistantRequest>(config)?
        .update_with_extra::<_, StateError>(id, |q, extra| {
            if q.version != version {
                return Ok(());
            }
            let mut w = waiting(extra)
                .filter(|w| w.version == version)
                .unwrap_or(Waiting {
                    version,
                    step: super::steps::current(q),
                    ..Waiting::default()
                });
            w.ended = true;
            w.resolved = true;
            set_waiting(extra, &w);
            Ok(())
        })
        .map_err(|e| super::state_error(&e))?;
    tracing::info!(request = id, version, "chain done");
    Ok(())
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
