//! `cancel-request` (ADR 0048 §6): the version is `cancel-requested` at once,
//! so a late result is not stored; its run's record is dropped and its
//! session closed as the daemon closes a Routine run's; once the session is
//! gone the version is `cancelled`. The daemon's tick retries a failed close.
//! A version whose last step waits on a hand-off keeps its results and
//! ends its chain there (ADR 0051 §3).
use std::sync::Arc;

use muxa_protocol::{AssistantRequest, Delivery, ErrorBody, ErrorCode, WorkItem};

use super::work::Fail;
use crate::convert::error_body;
use crate::task_records::{Config, Context};

/// What a cancel left to do.
enum Next {
    /// Nothing runs: the version is `cancelled`.
    Done,
    /// The run's session closes first.
    Close(String),
    /// The run is being made; the tick closes it once it exists.
    Wait,
}

pub async fn cancel(
    cx: &Context,
    request: String,
    version: u64,
) -> Result<(WorkItem, AssistantRequest), ErrorBody> {
    let config = Arc::clone(&cx.records);
    let id = request.clone();
    let (work, next) = super::blocking(move || {
        // A version whose last step waits on a hand-off: the chain ends there.
        match super::chain::end(&config, &id, version)? {
            Some(work) => Ok((work, Next::Done)),
            None => mark(&config, &id, version),
        }
    })
    .await?;
    if let Next::Close(task) = &next {
        let (config, t) = (Arc::clone(&cx.records), task.clone());
        super::blocking(move || {
            super::tick::drop_run(&config, &t, "The request was cancelled.");
            Ok(())
        })
        .await?;
        crate::task_records::close::close_ended(cx, task).await;
    }
    let gone = match &next {
        Next::Done => true,
        Next::Wait => false,
        Next::Close(task) => !super::tick::has_session(cx, task).await.unwrap_or(true),
    };
    let config = Arc::clone(&cx.records);
    let id = request.clone();
    super::blocking(move || {
        if gone {
            cancelled(&config, &id, version)?;
        }
        super::ask::reply(&config, &work, &id)
    })
    .await
}

/// Blocking. Marks the version `cancel-requested`; again is a no-op.
fn mark(config: &Config, id: &str, version: u64) -> Result<(String, Next), ErrorBody> {
    let requests = super::shelf::<AssistantRequest>(config)?;
    let at = crate::records::now();
    let marked = requests.update_with_extra(id, |q, extra| -> Result<_, Fail> {
        if q.version != version {
            return Err(error_body(
                ErrorCode::Conflict,
                format!(
                    "request {} is at version {}; list it again",
                    q.id, q.version
                ),
            )
            .into());
        }
        let step = super::steps::current(q);
        let next = match (&q.run, super::steps::creating(extra, version, step)) {
            (Some(run), _) => Next::Close(run.clone()),
            (None, Some(since)) if at.saturating_sub(since) < super::ask::CREATING_LIMIT => {
                Next::Wait
            }
            (None, _) => Next::Done,
        };
        match q.delivery {
            Delivery::Cancelled => return Ok((q.work.clone(), Next::Done)),
            Delivery::CancelRequested => return Ok((q.work.clone(), next)),
            Delivery::Resulted => {
                return Err(error_body(
                    ErrorCode::BadRequest,
                    format!(
                        "version {version} of request {} has its result; nothing to cancel",
                        q.id
                    ),
                )
                .into());
            }
            _ => {}
        }
        q.delivery = Delivery::CancelRequested;
        q.delivery_at = at;
        q.updated_at = at;
        super::steps::sync(q);
        Ok((q.work.clone(), next))
    })?;
    let marked =
        marked.ok_or_else(|| error_body(ErrorCode::BadRequest, format!("no request {id}")))?;
    tracing::info!(request = id, version, "assistant request cancel requested");
    Ok(marked)
}

/// Blocking. `cancel-requested` becomes `cancelled` once nothing runs.
pub fn cancelled(config: &Config, id: &str, version: u64) -> Result<(), ErrorBody> {
    let at = crate::records::now();
    let changed = super::shelf::<AssistantRequest>(config)?.update(id, |q| -> Result<_, Fail> {
        if q.version != version || q.delivery != Delivery::CancelRequested {
            return Ok(false);
        }
        q.delivery = Delivery::Cancelled;
        q.delivery_at = at;
        q.updated_at = at;
        super::steps::sync(q);
        Ok(true)
    })?;
    if changed == Some(true) {
        tracing::info!(request = id, version, "assistant request cancelled");
    }
    Ok(())
}
