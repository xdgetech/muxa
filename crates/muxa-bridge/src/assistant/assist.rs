//! `capture {assist}` (ADR 0048 §9): save the input, make a Work item from it
//! and ask the assistant, as `sort-capture work` and `request-assistant`
//! would. The input is saved first: a refused start keeps it unsorted.
use std::sync::Arc;

use muxa_protocol::{
    AssistSpec, AssistantRequest, Capture, CaptureState, ErrorBody, ErrorCode, SortAction, WorkItem,
};

use super::ask::{self, Ask};
use super::work::{self, Sort};
use crate::convert::error_body;
use crate::task_records::{Config, Context};

/// Without a usable daemon nothing processes a request (ADR 0048 §8).
pub const NO_DAEMON: &str = "Automatic processing is off: assistant requests run in the muxa \
                             daemon. On the host, run: muxa-bridge daemon install";

/// What `capture-saved` carries, and the error for its `result`.
pub struct Assisted {
    pub capture: Capture,
    pub work: Option<WorkItem>,
    pub request: Option<AssistantRequest>,
    pub error: Option<ErrorBody>,
}

impl Assisted {
    fn refused(capture: Capture, error: ErrorBody) -> Self {
        Self {
            capture,
            work: None,
            request: None,
            error: Some(error),
        }
    }
}

/// `capture` is saved already. A resend of a sorted one returns what the
/// first send made and starts nothing.
pub async fn assist(cx: &Context, capture: Capture, spec: AssistSpec) -> Assisted {
    if !cx.daemon {
        return Assisted::refused(capture, error_body(ErrorCode::Unsupported, NO_DAEMON));
    }
    let Some(workspace) = capture.workspace.clone() else {
        let e = error_body(ErrorCode::BadRequest, "capture: assist needs a workspace");
        return Assisted::refused(capture, e);
    };
    let config = Arc::clone(&cx.records);
    if capture.state == CaptureState::Sorted {
        let c = capture.clone();
        return match super::blocking(move || made(&config, &c, &workspace)).await {
            Ok((work, request)) => Assisted {
                capture,
                work,
                request,
                error: None,
            },
            Err(e) => Assisted::refused(capture, e),
        };
    }
    let (fallback, ws, c) = (cx.tasks.command.clone(), workspace.clone(), capture.clone());
    let worker = spec.worker.clone();
    let sorted = super::blocking(move || {
        ask::precheck(&config, &ws, worker.as_deref(), &fallback)?;
        let sort = Sort {
            capture: c.id.clone(),
            rev: c.rev,
            action: SortAction::Work,
            work: None,
            title: None,
            workspace: Some(ws),
        };
        work::sort(&config, sort)
    })
    .await;
    let (capture, item) = match sorted {
        Ok((c, Some(item))) => (c, item),
        Ok((c, None)) => {
            let e = error_body(ErrorCode::Internal, "the Capture made no Work item");
            return Assisted::refused(c, e);
        }
        Err(e) => return Assisted::refused(capture, e),
    };
    let a = Ask {
        work: item.id.clone(),
        rev: item.rev,
        scope: spec.scope,
        workspace: Some(workspace),
        purpose: spec.purpose,
        worker: spec.worker,
        asker: muxa_protocol::StepBy::User,
    };
    match ask::ask(cx, a).await {
        Ok((item, q, error)) => Assisted {
            capture,
            work: Some(item),
            request: Some(q),
            error,
        },
        Err(e) => Assisted {
            capture,
            work: Some(item),
            request: None,
            error: Some(e),
        },
    }
}

/// Blocking. The Work item a sorted Capture went to, and its request to `workspace`.
fn made(
    config: &Config,
    c: &Capture,
    workspace: &str,
) -> Result<(Option<WorkItem>, Option<AssistantRequest>), ErrorBody> {
    let Some(work) = c.work.first() else {
        return Ok((None, None));
    };
    let request = super::shelf::<AssistantRequest>(config)?
        .all()
        .map_err(|e| super::state_error(&e))?
        .into_iter()
        .find(|q| q.work == *work && q.workspace == workspace);
    match request {
        Some(q) => {
            let (item, q) = ask::reply(config, work, &q.id)?;
            Ok((Some(item), Some(q)))
        }
        None => {
            let item = super::shelf::<WorkItem>(config)?
                .get(work)
                .map_err(|e| super::state_error(&e))?
                .map(|w| super::listed(config, w));
            Ok((item, None))
        }
    }
}
