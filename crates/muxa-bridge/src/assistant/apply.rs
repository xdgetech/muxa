//! `apply-notes` and `notes = "auto"` (ADR 0047 §1): a stored proposal
//! replaces the workspace's live notes only while they still hash to its
//! base. Logs carry ids, sizes and hashes, never the text.
use std::path::PathBuf;

use muxa_protocol::{AssistantRequest, ErrorBody, ErrorCode, Notes, WorkItem};

use super::notes::{self, NotesError};
use super::record::RunFor;
use crate::convert::error_body;
use crate::records::Record;
use crate::records::assistant::NotesMode;
use crate::task_records::Config;

/// Hex digits of a hash a log line shows.
const SHOWN_HASH: usize = 12;

/// Blocking. Applies the notes `source`'s result of `version` proposed; the
/// Work item and the request, as a list shows them.
pub fn apply_notes(
    config: &Config,
    source: &str,
    version: u64,
    base: &str,
) -> Result<(WorkItem, AssistantRequest), ErrorBody> {
    if source.starts_with("p-") {
        return Err(error_body(
            ErrorCode::Unsupported,
            "this host keeps no plans yet; apply-notes takes a request (q-…) only",
        ));
    }
    if !is_request_id(source) {
        return Err(bad(
            "apply-notes: source must be a request id, q- and 8 hex digits",
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
    super::ask::reply(config, &q.work, &q.id)
}

/// Blocking. After a result with notes is stored: with `notes = "auto"`,
/// the host applies them. A changed base keeps the proposal for the user.
pub fn auto_apply(config: &Config, record: &Record, run: &RunFor, proposal: &Notes) {
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
    let (request, version, base) = (&run.request, run.version, shown(&proposal.base));
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

fn is_request_id(id: &str) -> bool {
    id.strip_prefix("q-")
        .is_some_and(|h| h.len() == 8 && h.bytes().all(|b| b.is_ascii_hexdigit()))
}

fn bad(message: impl Into<String>) -> ErrorBody {
    error_body(ErrorCode::BadRequest, message.into())
}

#[cfg(test)]
#[path = "apply_tests.rs"]
mod tests;
