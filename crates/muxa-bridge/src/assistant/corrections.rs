//! Corrections (ADR 0048 §2): what the host saw the user do while a plan was
//! current. Each is written after the change it describes, so a failed write
//! loses a signal, never the change. Reasons never reach a log.
use muxa_protocol::{
    Correction, CorrectionKind, Delivery, ErrorBody, ErrorCode, InboxAction, InboxItem, ItemKind,
    Plan, WorkAction, WorkItem,
};

use super::store::Shelf;
use crate::convert::error_body;
use crate::items::store::ItemStore;
use crate::task_records::Config;

/// Bytes of a reason.
pub const MAX_REASON: usize = 500;
/// Corrections kept, newest first (ADR 0048 §8).
pub const MAX_KEPT: usize = 200;
/// Seconds a correction is kept: 90 days.
pub const KEEP_FOR: u64 = 90 * 86_400;
/// Corrections in one `inbox-list`.
pub const MAX_LISTED: usize = 50;

/// What the user did, before the plan it is against is known.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Seen {
    pub kind: CorrectionKind,
    /// The Work item or Inbox item; None with `reordered`.
    pub row: Option<String>,
    pub workspace: Option<String>,
    pub before: Vec<String>,
    pub after: Vec<String>,
    /// Never logged.
    pub reason: Option<String>,
}

impl Seen {
    pub fn row(kind: CorrectionKind, row: &str, workspace: Option<String>) -> Self {
        Self {
            kind,
            row: Some(row.to_owned()),
            workspace,
            before: Vec::new(),
            after: Vec::new(),
            reason: None,
        }
    }
}

/// The Work item or Inbox item a plan row names.
pub fn row_id(row: &muxa_protocol::PlanRow) -> Option<&str> {
    row.work.as_deref().or(row.item.as_deref())
}

/// Whether `plan`'s `top` lists `row`.
pub fn listed(plan: &Plan, row: &str) -> bool {
    plan.top.iter().any(|r| row_id(r) == Some(row))
}

/// The newest `resulted` plan in `plans`: the one the user sees as Today.
pub fn newest_resulted(mut plans: Vec<Plan>) -> Option<Plan> {
    super::plan::newest_first(&mut plans);
    plans.into_iter().find(|p| p.delivery == Delivery::Resulted)
}

/// Blocking. The current plan; None without one, or when the plans cannot
/// be read (logged).
fn current_plan(config: &Config) -> Option<Plan> {
    if !config.state_dir.as_ref().is_some_and(|d| d.is_dir()) {
        return None;
    }
    let plans = super::plan::shelf(config).ok()?.all();
    match plans {
        Ok(plans) => newest_resulted(plans),
        Err(e) => {
            tracing::warn!(error = %e, "cannot read the plans for a correction");
            None
        }
    }
}

/// Blocking. Records `seen` against `plan`. A failed write is logged.
pub fn record(config: &Config, plan: &Plan, seen: Seen, now: u64) -> Option<Correction> {
    let in_top = match &seen.row {
        Some(row) => listed(plan, row),
        None => true,
    };
    let made = super::shelf::<Correction>(config).and_then(|shelf| {
        shelf
            .create(|id| {
                Ok::<_, super::work::Fail>(Correction {
                    id,
                    kind: seen.kind,
                    at: now,
                    plan: plan.id.clone(),
                    row: seen.row,
                    in_top,
                    workspace: seen.workspace,
                    before: seen.before,
                    after: seen.after,
                    reason: seen.reason,
                })
            })
            .map_err(ErrorBody::from)
    });
    match made {
        Ok(c) => {
            tracing::info!(correction = %c.id, kind = ?c.kind, plan = %c.plan, in_top, "correction recorded");
            Some(c)
        }
        Err(e) => {
            tracing::warn!(plan = %plan.id, kind = ?seen.kind, error = %e.message, "cannot record a correction");
            None
        }
    }
}

/// Blocking. After `update-work`: a `later`, `pin` or `unpin` that changed
/// the item is a correction of the current plan.
pub fn after_work(config: &Config, item: &WorkItem, action: WorkAction, changed: bool, now: u64) {
    let kind = match action {
        WorkAction::Later => CorrectionKind::Deferred,
        WorkAction::Pin => CorrectionKind::Pinned,
        WorkAction::Unpin => CorrectionKind::Unpinned,
        _ => return,
    };
    if !changed {
        return;
    }
    if let Some(plan) = current_plan(config) {
        let seen = Seen::row(kind, &item.id, item.workspace.clone());
        record(config, &plan, seen, now);
    }
}

/// Blocking. After `update-inbox` on an item: `reject` of a `sort`,
/// `delegate` or `hypothesis` item, and `later` on an item the current
/// plan listed.
pub fn after_answer(
    config: &Config,
    item: &InboxItem,
    action: InboxAction,
    reason: Option<String>,
    now: u64,
) {
    let proposal = matches!(
        item.kind,
        ItemKind::Sort | ItemKind::Delegate | ItemKind::Hypothesis
    );
    let kind = match action {
        InboxAction::Reject if proposal => CorrectionKind::Rejected,
        InboxAction::Later => CorrectionKind::Deferred,
        _ => return,
    };
    let Some(plan) = current_plan(config) else {
        return;
    };
    if kind == CorrectionKind::Deferred && !listed(&plan, &item.id) {
        return;
    }
    let seen = Seen {
        reason,
        ..Seen::row(kind, &item.id, Some(item.workspace.clone()))
    };
    record(config, &plan, seen, now);
}

/// Blocking. The workspace of a row: its Work item's, or its Inbox item's.
pub fn row_workspace(config: &Config, row: &str) -> Option<String> {
    let dir = config.state_dir.as_ref()?;
    if row.starts_with("w-") {
        return super::shelf::<WorkItem>(config)
            .ok()?
            .get(row)
            .ok()??
            .workspace;
    }
    let store = ItemStore::open(dir).ok()?;
    let task = store.owner(row).ok()??;
    let items = store.get(&task).ok()?;
    let item = items.items.into_iter().find(|s| s.item.id == row)?;
    Some(item.item.workspace)
}

/// An optional reason: at most [`MAX_REASON`] bytes, with line breaks and
/// tabs the only control characters. Empty is none. The error never quotes it.
pub fn check_reason(text: Option<&str>) -> Result<Option<String>, ErrorBody> {
    let Some(text) = text.filter(|t| !t.trim().is_empty()) else {
        return Ok(None);
    };
    let controls = text
        .chars()
        .any(|c| c.is_control() && !matches!(c, '\n' | '\r' | '\t'));
    if text.len() > MAX_REASON || controls {
        return Err(error_body(
            ErrorCode::BadRequest,
            format!(
                "a reason is at most {MAX_REASON} bytes, with line breaks and tabs the only \
                 control characters"
            ),
        ));
    }
    Ok(Some(text.to_owned()))
}

/// Blocking. The corrections kept, newest first: at most [`MAX_KEPT`], none
/// older than [`KEEP_FOR`]. Empty without a state directory.
pub fn live(config: &Config, now: u64) -> Result<Vec<Correction>, ErrorBody> {
    if !config.state_dir.as_ref().is_some_and(|d| d.is_dir()) {
        return Ok(Vec::new());
    }
    let all = super::shelf::<Correction>(config)?
        .all()
        .map_err(|e| super::state_error(&e))?;
    Ok(kept(all, now).0)
}

/// Blocking. What `inbox-list` carries: the newest [`MAX_LISTED`].
pub fn list(config: &Config, now: u64) -> Result<Vec<Correction>, ErrorBody> {
    let mut all = live(config, now)?;
    all.truncate(MAX_LISTED);
    Ok(all)
}

/// Newest first: the ones kept, then the ones out.
fn kept(mut all: Vec<Correction>, now: u64) -> (Vec<Correction>, Vec<Correction>) {
    all.sort_by(|a, b| (b.at, &b.id).cmp(&(a.at, &a.id)));
    let (mut keep, mut out): (Vec<_>, Vec<_>) = all
        .into_iter()
        .partition(|c| now.saturating_sub(c.at) <= KEEP_FOR);
    if keep.len() > MAX_KEPT {
        out.extend(keep.split_off(MAX_KEPT));
    }
    (keep, out)
}

/// Blocking. Removes the corrections past [`KEEP_FOR`] or beyond
/// [`MAX_KEPT`], oldest out first; the daemon's tick calls it. A failure is
/// logged and tried again next tick.
pub fn prune(config: &Config, now: u64) {
    let Some(dir) = config.state_dir.as_ref().filter(|d| d.is_dir()) else {
        return;
    };
    let shelf = match Shelf::<Correction>::open(dir) {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!(error = %e, "cannot open the corrections");
            return;
        }
    };
    let Ok(all) = shelf.all() else { return };
    let (_, out) = kept(all, now);
    for c in out {
        match shelf.remove(&c.id) {
            Ok(()) => tracing::info!(correction = %c.id, "old correction removed"),
            Err(e) => tracing::warn!(correction = %c.id, error = %e, "cannot remove a correction"),
        }
    }
}

#[cfg(test)]
#[path = "corrections_tests.rs"]
pub(crate) mod tests;
