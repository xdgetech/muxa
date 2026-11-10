//! The workspace memory's copy in a run's folder (ADR 0053 §2): checked
//! before the agent starts, written beside `TASK.md`, refreshed at each
//! restart. Logs carry the Task and the hash, never the text.
use std::path::{Path, PathBuf};

use muxa_protocol::ErrorBody;

use super::Config;
use crate::assistant::notes;
use crate::records::Record;

/// Blocking. The root of the record's workspace; None when the host no
/// longer lists it.
pub(super) fn root_of(config: &Config, record: &Record) -> Result<Option<PathBuf>, ErrorBody> {
    Ok(config
        .load_workspaces()?
        .get(&record.workspace)
        .map(|w| w.root.clone()))
}

/// Blocking. Refuses a start while the live file is not one a run may read.
pub(super) fn check(root: &Path) -> Result<(), String> {
    notes::read(root).map(drop).map_err(|e| e.to_string())
}

/// Blocking. The copy for `record`'s run in `dir`. An assistant run got its
/// copy, with the hash it may propose against, before its agent started.
pub(super) fn give(record: &Record, root: &Path, dir: &Path) -> Result<(), String> {
    if record.is_assistant_run() {
        return Ok(());
    }
    let copied = notes::give(root, dir).map_err(|e| e.to_string())?;
    match copied {
        Some(hash) => tracing::info!(task = %record.id, hash = %hash, "workspace memory copied"),
        None => tracing::debug!(task = %record.id, "no workspace memory to copy"),
    }
    Ok(())
}
