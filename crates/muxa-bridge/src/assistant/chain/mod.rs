//! What follows a step's hand-off (ADR 0051 §3). Under `handoffs = "review"`
//! the daemon's tick makes one `handoff` Inbox item in the asking step's run
//! file; under `auto` it starts the next step within every budget and
//! `max_auto_handoffs_per_day`, else makes the item with the limit. A new
//! version or a cancel ends a waiting chain and resolves its item. Logs
//! carry ids, steps and worker names, never a purpose.
pub mod items;
pub mod next;

use std::sync::Arc;

use muxa_protocol::{AssistantRequest, Delivery, Handoff};
use serde::{Deserialize, Serialize};

use super::store::{Extra, Shelf};
use crate::records::Workspaces;
use crate::records::staff::Handoffs;
use crate::routines::store::StateError;
use crate::task_records::{Config, Context};

/// The request file's mark of a chain that waits on a hand-off, or ended.
pub const WAITING_FIELD: &str = "handoffWaiting";

/// A chain that waits on step `step`'s hand-off.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Waiting {
    pub version: u64,
    pub step: u32,
    /// The run whose item file holds the `handoff` item. None: no item yet.
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

/// The step whose stored result asks for a hand-off that has not started
/// yet, with the hand-off. None for a request without staff, or a version
/// whose newest step is running, failed or cancelled.
pub fn pending(q: &AssistantRequest) -> Option<(u32, &Handoff)> {
    let last = q.steps.last()?;
    let answered = q
        .results
        .iter()
        .any(|r| r.version == q.version && r.step.unwrap_or(1) == last.step);
    if q.delivery != Delivery::Resulted || !answered {
        return None;
    }
    Some((last.step, last.handoff.as_ref()?))
}

/// Whether the chain of `version` ended without its next step.
pub fn ended(extra: &Extra, version: u64) -> bool {
    waiting(extra).is_some_and(|w| w.ended && w.version == version)
}

/// What the tick does for one request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Todo {
    /// `handoffs = "review"`: the item.
    Ask { step: u32 },
    /// `handoffs = "auto"`: the next step.
    Start { step: u32 },
    /// The waiting item no longer applies.
    Resolve { task: String, why: &'static str },
}

/// Pure. What one request's chain needs now.
pub(super) fn todo(q: &AssistantRequest, extra: &Extra, mode: Option<Handoffs>) -> Option<Todo> {
    let mark = waiting(extra);
    if let Some(w) = &mark {
        let gone = if w.version != q.version {
            Some("A new version of the request ended this chain.")
        } else if matches!(q.delivery, Delivery::CancelRequested | Delivery::Cancelled) || w.ended {
            Some("The request was cancelled; its chain ended.")
        } else {
            None
        };
        if let (Some(why), Some(task), false) = (gone, &w.task, w.resolved) {
            return Some(Todo::Resolve {
                task: task.clone(),
                why,
            });
        }
    }
    let (step, _) = pending(q)?;
    if mark.is_some_and(|w| w.version == q.version && w.step == step) {
        return None;
    }
    match mode? {
        Handoffs::Review => Some(Todo::Ask { step }),
        Handoffs::Auto => Some(Todo::Start { step }),
        // The check refuses a hand-off then; configuration changed since.
        Handoffs::None => None,
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
    let mut starts = Vec::new();
    for q in all.into_iter().filter(|q| !q.steps.is_empty()) {
        let Ok(Some((q, extra))) = shelf.get_with_extra(&q.id) else {
            continue;
        };
        match todo(&q, &extra, mode(&workspaces, &q.workspace)) {
            Some(Todo::Ask { step }) => ask(config, &shelf, &q, step, None),
            Some(Todo::Start { step }) => starts.push((q.id.clone(), q.version, step)),
            Some(Todo::Resolve { task, why }) => resolve(config, &shelf, &q, &task, why),
            None => {}
        }
    }
    starts
}

fn mode(workspaces: &Workspaces, workspace: &str) -> Option<Handoffs> {
    let d = workspaces.get(workspace)?.assistant.as_ref()?;
    Some(d.staff.handoffs)
}

/// Blocking. The `handoff` item of `step`, once, then the mark that it waits.
/// `summary` says why the host did not start the step itself.
pub(super) fn ask(
    config: &Config,
    shelf: &Shelf<AssistantRequest>,
    q: &AssistantRequest,
    step: u32,
    summary: Option<String>,
) {
    let Some(task) = q
        .steps
        .iter()
        .find(|s| s.step == step)
        .and_then(|s| s.run.clone())
    else {
        tracing::warn!(request = %q.id, version = q.version, step, "a hand-off's step has no run");
        return;
    };
    match items::make(config, &task, q, step, summary) {
        Ok(Some(item)) => {
            tracing::info!(request = %q.id, version = q.version, step, item, "hand-off item made")
        }
        Ok(None) => {}
        Err(e) => {
            tracing::warn!(request = %q.id, step, error = %e.message, "cannot make a hand-off item");
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

/// Blocking. Closes the waiting item, then marks it closed.
fn resolve(
    config: &Config,
    shelf: &Shelf<AssistantRequest>,
    q: &AssistantRequest,
    task: &str,
    why: &str,
) {
    if let Err(e) = items::resolve(config, task, why) {
        tracing::warn!(request = %q.id, task, error = %e.message, "cannot resolve a hand-off item");
        return;
    }
    tracing::info!(request = %q.id, version = q.version, task, "hand-off item resolved");
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
        let summary = format!("Not started automatically: {why} Approve to start it.");
        if q.version == version && pending(&q).is_some_and(|(s, _)| s == step) {
            ask(&config, &shelf, &q, step, Some(summary));
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
        resolve(
            config,
            &shelf,
            &q,
            task,
            "The request was cancelled; its chain ended.",
        );
    }
    Ok(Some(q.work))
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
