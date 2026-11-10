//! `apply-notes` and `notes = "auto"` (ADR 0049 §1): a stored proposal
//! replaces the workspace's live notes only while they still hash to its
//! base. Logs carry ids, sizes and hashes, never the text.
use std::path::PathBuf;

use muxa_protocol::{AssistantRequest, ErrorBody, ErrorCode, Notes, Plan, WorkItem};

use super::notes::{self, NotesError};
use crate::convert::error_body;
use crate::records::Record;
use crate::records::assistant::NotesMode;
use crate::task_records::Config;

/// Hex digits of a hash a log line shows.
const SHOWN_HASH: usize = 12;

/// What `apply-notes` replies with.
#[derive(Debug)]
pub enum Applied {
    /// The Work item and the request, as a list shows them.
    Request(WorkItem, AssistantRequest),
    /// The plan, as `plans[]` lists it.
    Plan(Plan),
}

/// Blocking. Applies the notes that `source` proposed: a request's result
/// of `version`, or a plan at `rev` `version`.
pub fn apply_notes(
    config: &Config,
    source: &str,
    version: u64,
    base: &str,
) -> Result<Applied, ErrorBody> {
    if is_id(source, "p-") {
        return apply_plan(config, source, version, base).map(Applied::Plan);
    }
    if !is_id(source, "q-") {
        return Err(bad(
            "apply-notes: source must be a request (q-…) or a plan (p-…) id",
        ));
    }
    let q = super::shelf::<AssistantRequest>(config)?
        .get(source)
        .map_err(|e| super::state_error(&e))?
        .ok_or_else(|| bad(format!("no request {source}")))?;
    let proposal = q
        .results
        .iter()
        .find(|r| r.version == version)
        .and_then(|r| r.notes.clone())
        .ok_or_else(|| {
            bad(format!(
                "request {source} has no proposed notes at version {version}"
            ))
        })?;
    if proposal.base != base {
        return Err(bad(
            "apply-notes: base is not the proposal's; list the request again",
        ));
    }
    let root = workspace_root(config, &q.workspace)?;
    notes::apply(&root, &proposal.text, &proposal.base).map_err(|e| {
        tracing::info!(request = %q.id, version, base = shown(base), error = %e, "notes not applied");
        refused(&e)
    })?;
    tracing::info!(request = %q.id, version, base = shown(base), bytes = proposal.text.len(), "notes applied");
    super::ask::reply(config, &q.work, &q.id).map(|(w, q)| Applied::Request(w, q))
}

/// The overall notes a plan proposed, written to the overall workspace's root.
fn apply_plan(config: &Config, id: &str, rev: u64, base: &str) -> Result<Plan, ErrorBody> {
    let (plan, extra) = super::plan::shelf(config)?
        .get_with_extra(id)
        .map_err(|e| super::state_error(&e))?
        .ok_or_else(|| bad(format!("no plan {id}")))?;
    if plan.rev != rev {
        return Err(error_body(
            ErrorCode::Conflict,
            format!("plan {id} is at rev {}; list it again", plan.rev),
        ));
    }
    let proposal = plan
        .notes
        .clone()
        .ok_or_else(|| bad(format!("plan {id} has no proposed notes")))?;
    if proposal.base != base {
        return Err(bad(
            "apply-notes: base is not the proposal's; list the plan again",
        ));
    }
    let workspace = extra
        .get(super::plan::WORKSPACE_FIELD)
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| bad(format!("plan {id} names no overall workspace")))?;
    let root = workspace_root(config, workspace)?;
    notes::apply(&root, &proposal.text, &proposal.base).map_err(|e| {
        tracing::info!(plan = id, rev, base = shown(base), error = %e, "notes not applied");
        refused(&e)
    })?;
    tracing::info!(
        plan = id,
        rev,
        base = shown(base),
        bytes = proposal.text.len(),
        "notes applied"
    );
    Ok(super::plan::listed(config, plan))
}

/// Blocking. After a result with notes is stored: with `notes = "auto"`,
/// the host applies them. A changed base keeps the proposal for the user.
/// `source` and `version` are the request and its version, or the plan and its `rev`.
pub fn auto_apply(config: &Config, record: &Record, source: &str, version: u64, proposal: &Notes) {
    let workspaces = match config.load_workspaces() {
        Ok(w) => w,
        Err(_) => return,
    };
    let Some(workspace) = workspaces.get(&record.workspace) else {
        return;
    };
    let auto = workspace
        .assistant
        .as_ref()
        .is_some_and(|d| d.notes == NotesMode::Auto);
    if !auto {
        return;
    }
    let (request, base) = (source, shown(&proposal.base));
    match notes::apply(&workspace.root, &proposal.text, &proposal.base) {
        Ok(()) => tracing::info!(
            task = %record.id, request = %request, version, base,
            bytes = proposal.text.len(), "notes applied at publish"
        ),
        Err(NotesError::Changed { .. }) => tracing::info!(
            task = %record.id, request = %request, version, base,
            "notes kept for review: base changed"
        ),
        Err(e) => tracing::warn!(
            task = %record.id, request = %request, version, error = %e,
            "cannot apply notes at publish"
        ),
    }
}

/// Blocking. The root of a configured workspace.
pub fn workspace_root(config: &Config, name: &str) -> Result<PathBuf, ErrorBody> {
    config
        .load_workspaces()?
        .get(name)
        .map(|w| w.root.clone())
        .ok_or_else(|| bad(format!("no workspace {name}")))
}

/// The error a client sees for notes that cannot be read or written.
pub fn refused(e: &NotesError) -> ErrorBody {
    let code = match e {
        NotesError::Changed { .. } => ErrorCode::Conflict,
        NotesError::Io { .. } => ErrorCode::Internal,
        _ => ErrorCode::BadRequest,
    };
    error_body(code, e.to_string())
}

fn shown(hash: &str) -> &str {
    hash.get(..SHOWN_HASH).unwrap_or(hash)
}

fn is_id(id: &str, prefix: &str) -> bool {
    id.strip_prefix(prefix)
        .is_some_and(|h| h.len() == 8 && h.bytes().all(|b| b.is_ascii_hexdigit()))
}

fn bad(message: impl Into<String>) -> ErrorBody {
    error_body(ErrorCode::BadRequest, message.into())
}

#[cfg(test)]
#[path = "apply_tests.rs"]
mod tests;
