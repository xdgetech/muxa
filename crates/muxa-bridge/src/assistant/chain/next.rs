//! Starts the step a hand-off asks for (ADR 0051 §3): approved by the user
//! or started by the host under `handoffs = "auto"`. The step is claimed in
//! the request's write that adds it, so one `(request, version, step)` makes
//! one run; a later call finds the step and its run again, by the `s<k>`
//! marker after a crash.
use std::sync::Arc;

use muxa_protocol::{AssistantRequest, Delivery, ErrorBody, ErrorCode, Handoff, StepBy};

use super::{ended, pending};
use crate::assistant::ask::{self, Asked, CLAIMS, CREATING_FIELD};
use crate::assistant::policy::{self, Delegated};
use crate::assistant::store::Extra;
use crate::assistant::work::Fail;
use crate::assistant::{blocking, shelf, state_error, steps};
use crate::convert::error_body;
use crate::records::Workspaces;
use crate::task_records::{Config, Context};

/// The request file's `auto` steps, as epoch seconds: they count against
/// `[assistant] max_auto_handoffs_per_day`.
pub const AUTO_FIELD: &str = "autoHandoffs";
/// Seconds an `auto` start stays in [`AUTO_FIELD`]: longer than any day.
const AUTO_KEPT: u64 = 2 * 86_400;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Via {
    /// The user approved the `handoff` item.
    Approved,
    /// `handoffs = "auto"`: the host, within `max_auto_handoffs_per_day`.
    Auto,
}

pub(super) enum Claimed {
    /// The next step, claimed for its run.
    Start(Box<AssistantRequest>, Box<Delegated>),
    /// The step exists, with its run or one being made.
    Known(String),
}

/// Starts step `step + 1` of `request`'s `version`. The Work item and the
/// request to reply with, and an error when the run did not start.
pub async fn start(
    cx: &Context,
    request: String,
    version: u64,
    step: u32,
    via: Via,
) -> Result<Asked, ErrorBody> {
    let (config, fallback, id) = (
        Arc::clone(&cx.records),
        cx.tasks.command.clone(),
        request.clone(),
    );
    let at = crate::records::now();
    let claimed = blocking(move || claim(&config, &fallback, &id, version, step, via, at)).await?;
    let (work, error) = match claimed {
        Claimed::Start(q, d) => {
            tracing::info!(request = %q.id, version, step = step + 1, via = ?via, "hand-off step claimed");
            (q.work.clone(), ask::start_run(cx, *q, &d).await)
        }
        Claimed::Known(work) => (work, None),
    };
    let config = Arc::clone(&cx.records);
    let (item, q) = blocking(move || ask::reply(&config, &work, &request)).await?;
    Ok((item, q, error))
}

/// Blocking. Under [`CLAIMS`]: the next step claimed, or found.
pub(super) fn claim(
    config: &Config,
    fallback: &str,
    id: &str,
    version: u64,
    step: u32,
    via: Via,
    at: u64,
) -> Result<Claimed, ErrorBody> {
    let workspaces = config.load_workspaces()?;
    let requests = shelf::<AssistantRequest>(config)?;
    let (q, extra) = requests
        .get_with_extra(id)
        .map_err(|e| state_error(&e))?
        .ok_or_else(|| bad(format!("no request {id}")))?;
    let next = step + 1;
    let made = q
        .steps
        .iter()
        .find(|s| s.step == next)
        .map(|s| s.worker.clone());
    let worker = match made.clone() {
        Some(w) => w,
        None => wanted(&q, &extra, version, step)?.worker,
    };
    let d = policy::delegated(&workspaces, &q.workspace, Some(&worker), fallback)?;
    let _one = CLAIMS
        .lock()
        .map_err(|_| error_body(ErrorCode::Internal, "the assistant claim lock broke"))?;
    if made.is_some() {
        if q.version != version || steps::current(&q) != next {
            return Ok(Claimed::Known(q.work));
        }
        return Ok(match ask::claim_run(config, &d, id, version, at)? {
            Some(q) => Claimed::Start(Box::new(q), Box::new(d)),
            None => Claimed::Known(q.work),
        });
    }
    if via == Via::Auto {
        auto_budget(config, &workspaces, at)?;
    }
    let claimed = requests.update_with_extra(id, |q, extra| -> Result<_, Fail> {
        let h = wanted(q, extra, version, step)?;
        if next > d.delegate.staff.max_steps {
            return Err(bad(format!(
                "request {} is at step {step}, its workspace's max_steps",
                q.id
            ))
            .into());
        }
        ask::budget(config, &d, &[q.id.as_str()], at)?;
        q.steps
            .push(steps::fresh(next, &h.worker, StepBy::Handoff, at));
        q.run = None;
        q.delivery = Delivery::Pending;
        q.delivery_at = at;
        q.updated_at = at;
        extra.insert(CREATING_FIELD.into(), steps::claim(version, next, at));
        if via == Via::Auto {
            count_auto(extra, at);
        }
        Ok(q.clone())
    })?;
    let q = claimed.ok_or_else(|| bad(format!("no request {id}")))?;
    Ok(Claimed::Start(Box::new(q), Box::new(d)))
}

/// The hand-off that step `step` of `version` waits on, or why none does.
fn wanted(
    q: &AssistantRequest,
    extra: &Extra,
    version: u64,
    step: u32,
) -> Result<Handoff, ErrorBody> {
    if q.version != version {
        return Err(error_body(
            ErrorCode::Conflict,
            format!(
                "request {} is at version {}; its new version ended this chain",
                q.id, q.version
            ),
        ));
    }
    let cancelled = matches!(q.delivery, Delivery::CancelRequested | Delivery::Cancelled);
    if cancelled || ended(extra, version) {
        return Err(bad(format!(
            "request {} was cancelled; its chain ended",
            q.id
        )));
    }
    match pending(q) {
        Some((s, h)) if s == step => Ok(h.clone()),
        _ => Err(bad(format!(
            "step {step} of request {} has no hand-off waiting",
            q.id
        ))),
    }
}

/// Blocking, under [`CLAIMS`]. Refuses an `auto` step past
/// `[assistant] max_auto_handoffs_per_day`.
fn auto_budget(config: &Config, workspaces: &Workspaces, at: u64) -> Result<(), ErrorBody> {
    let limit = workspaces.assistant().max_auto_handoffs_per_day;
    let (day_start, reset) = policy::day(at);
    let requests = shelf::<AssistantRequest>(config)?;
    let mut started = 0;
    for q in requests.all().map_err(|e| state_error(&e))? {
        if let Ok(Some((_, extra))) = requests.get_with_extra(&q.id) {
            started += autos(&extra).iter().filter(|t| **t >= day_start).count();
        }
    }
    if started >= limit as usize {
        return Err(error_body(
            ErrorCode::Conflict,
            format!(
                "this host started {started} hand-offs without review today, its [assistant] \
                 max_auto_handoffs_per_day. The count resets at {reset}."
            ),
        ));
    }
    Ok(())
}

fn autos(extra: &Extra) -> Vec<u64> {
    extra
        .get(AUTO_FIELD)
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or_default()
}

fn count_auto(extra: &mut Extra, at: u64) {
    let mut all = autos(extra);
    all.retain(|t| at.saturating_sub(*t) < AUTO_KEPT);
    all.push(at);
    extra.insert(AUTO_FIELD.into(), all.into());
}

fn bad(message: String) -> ErrorBody {
    error_body(ErrorCode::BadRequest, message)
}
