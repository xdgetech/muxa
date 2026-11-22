//! The Task records of protocol v1, from before F5: `<state dir>/t-<id>.json`.
//! `migrate-v1` reads the open ones (ADR 0076). Nothing here writes,
//! moves or deletes a record, a worktree or a branch.

use std::path::{Path, PathBuf};

use muxa_protocol::v2::{V1Status, V1Task};
use muxa_server::TaskError;
use serde::Deserialize;

use super::adopt;
use crate::records::{Workspaces, is_valid_id};

/// v1 never wrote a bigger record.
const MAX_FILE: u64 = 64 * 1024;

/// The fields of a v1 record that migration needs. v1 wrote camelCase.
#[derive(Debug, Clone, Deserialize)]
struct Record {
    id: String,
    workspace: String,
    title: String,
    status: String,
    /// The Task's folder or worktree.
    #[serde(default, alias = "worktree")]
    dir: Option<PathBuf>,
    /// A Routine run, not a Task (ADR 0048).
    #[serde(default)]
    kind: Option<serde_json::Value>,
}

impl Record {
    /// Not done or dropped. `review` was still open work in v1.
    fn is_open(&self) -> bool {
        self.kind.is_none() && matches!(self.status.as_str(), "open" | "review")
    }
}

/// The open v1 Tasks in `dir`, by id, each with what migration can do with
/// it: `new` when its place can be adopted.
pub async fn tasks(dir: Option<&Path>, workspaces: &Workspaces) -> Result<Vec<V1Task>, TaskError> {
    let Some(dir) = dir.map(Path::to_owned) else {
        return Ok(Vec::new());
    };
    let records = tokio::task::spawn_blocking(move || open_records(&dir))
        .await
        .map_err(|e| TaskError::Failed(e.to_string()))?
        .map_err(TaskError::Failed)?;
    let mut out = Vec::with_capacity(records.len());
    for r in records {
        out.push(task(r, workspaces).await?);
    }
    Ok(out)
}

async fn task(r: Record, workspaces: &Workspaces) -> Result<V1Task, TaskError> {
    let path = r.dir.as_deref().map(Path::to_owned).unwrap_or_default();
    let exists = r.dir.as_deref().is_some_and(Path::is_dir);
    let mut task = V1Task {
        id: r.id,
        workspace: r.workspace,
        title: r.title,
        path: path.to_string_lossy().into_owned(),
        exists,
        status: V1Status::Missing,
    };
    let Some(ws) = workspaces.get(&task.workspace) else {
        task.status = V1Status::NoWorkspace;
        return Ok(task);
    };
    if !exists {
        return Ok(task);
    }
    task.status = match adopt::resolve(ws, &task.path).await {
        Ok(a) => {
            task.path = a.path.to_string_lossy().into_owned();
            V1Status::New
        }
        Err(TaskError::Invalid(_)) => V1Status::Outside,
        Err(e) => return Err(e),
    };
    Ok(task)
}

/// The open records, by id. A record that cannot be read is logged by its
/// file name and skipped; a missing folder is no records. Blocking.
fn open_records(dir: &Path) -> Result<Vec<Record>, String> {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(format!("cannot read {}: {e}", dir.display())),
    };
    let mut out: Vec<Record> = entries
        .filter_map(Result::ok)
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            let id = name.strip_suffix(".json").filter(|id| is_valid_id(id))?;
            read(&e.path(), id)
        })
        .filter(Record::is_open)
        .collect();
    out.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(out)
}

fn read(path: &Path, id: &str) -> Option<Record> {
    let meta = std::fs::symlink_metadata(path).ok()?;
    if !meta.is_file() || meta.len() > MAX_FILE {
        tracing::warn!(record = id, "a v1 record is not a small file; skipped");
        return None;
    }
    let bytes = match std::fs::read(path) {
        Ok(b) => b,
        Err(e) => {
            tracing::warn!(record = id, error = %e, "a v1 record cannot be read; skipped");
            return None;
        }
    };
    match serde_json::from_slice::<Record>(&bytes) {
        Ok(r) if r.id == id => Some(r),
        Ok(_) => {
            tracing::warn!(record = id, "a v1 record names another id; skipped");
            None
        }
        Err(e) => {
            // The position only: serde's message can quote the record's text.
            tracing::warn!(
                record = id,
                line = e.line(),
                column = e.column(),
                "a v1 record is not valid; skipped"
            );
            None
        }
    }
}

#[cfg(test)]
#[path = "v1_tests.rs"]
pub(crate) mod tests;
