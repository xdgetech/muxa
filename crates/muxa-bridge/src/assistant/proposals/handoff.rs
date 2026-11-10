//! `update-inbox approve` on a `handoff` item (ADR 0051 §3): starts the next
//! step once, keyed by the item's id and proposal hash as a delegate's
//! approval is. A second approve returns the step the first made; a budget
//! reached is `conflict` and leaves the item open.
use std::sync::Arc;

use muxa_protocol::{ErrorBody, ErrorCode};

use super::approve::{Approved, By, Claim, Found, Ids, Made, again, bad, claim, finish, release};
use crate::assistant::chain::next::{self, Via};
use crate::assistant::{NO_DAEMON, blocking};
use crate::convert::error_body;
use crate::records::now;
use crate::task_records::Context;

pub(super) async fn approve(cx: &Context, f: Found) -> Result<Approved, ErrorBody> {
    let p = f
        .item
        .handoff
        .clone()
        .ok_or_else(|| bad("the item has no hand-off"))?;
    if !cx.daemon {
        return Err(error_body(ErrorCode::Unsupported, NO_DAEMON));
    }
    let (config, found, at) = (Arc::clone(&cx.records), f.clone(), now());
    let claimed = blocking(move || claim(&config, &found, p.version, By::User, at)).await?;
    if let Claim::Done(done) = claimed {
        let config = Arc::clone(&cx.records);
        return blocking(move || again(&config, &done)).await;
    }
    let config = Arc::clone(&cx.records);
    match next::start(cx, p.request.clone(), p.version, p.step, Via::Approved).await {
        Ok((w, q, error)) => {
            let ids = Ids {
                capture: None,
                work: Some(w.id.clone()),
                request: Some(q.id.clone()),
            };
            let saved = f.clone();
            let item = blocking(move || finish(&config, &saved, ids, By::User, now())).await?;
            tracing::info!(item = %f.item.id, request = %q.id, version = q.version, step = p.step + 1, worker = %p.worker, "hand-off approved");
            Ok(Approved {
                made: Some(Made::Asked(w, q)),
                item,
                error,
            })
        }
        Err(e) => {
            let released = blocking(move || {
                release(&config, &f, None, now());
                Ok(())
            })
            .await;
            if let Err(r) = released {
                tracing::warn!(error = %r.message, "cannot release a hand-off's approval");
            }
            Err(e)
        }
    }
}
