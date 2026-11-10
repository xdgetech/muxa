//! A Task's close-out into the workspace memory (ADR 0053 §3): the hash a
//! `memory` item's approve binds to, the heading that makes an append
//! idempotent, and the append under the workspace's OS lock. `closeout`
//! makes the item or appends at a Task's end. Logs carry ids, sizes and
//! hashes, never the text, the file or a heading.
pub mod closeout;

use std::fs::DirBuilder;
use std::os::unix::fs::DirBuilderExt;
use std::path::Path;

use muxa_protocol::MemoryProposal;
use sha2::{Digest, Sha256};

use crate::assistant::notes::{self, Appended, NotesError};
use crate::records::store::Lock;
use crate::task_records::Config;

/// Bytes of a section that approve may append.
pub const MAX_SECTION: usize = 4 << 10;
/// Under the state directory: one lock file per workspace root.
const LOCKS: &str = "workspaces";

#[derive(Debug, thiserror::Error)]
pub enum AppendError {
    #[error(transparent)]
    Notes(#[from] NotesError),
    #[error("the workspace memory is being written; try again")]
    Busy,
    #[error("cannot lock the workspace memory: {0}")]
    Lock(String),
}

/// `sha256:` and the hex SHA-256 of `task`, NUL, `title`, NUL, `text`.
pub fn hash(task: &str, title: &str, text: &str) -> String {
    let mut h = Sha256::new();
    for (i, part) in [task, title, text].iter().enumerate() {
        if i > 0 {
            h.update([0]);
        }
        h.update(part.as_bytes());
    }
    let hex: String = h.finalize().iter().map(|b| format!("{b:02x}")).collect();
    format!("sha256:{hex}")
}

/// Whether approve may append `text`: at most 4 KiB, with no character the
/// section reader replaced.
pub fn approvable(text: &str) -> bool {
    !text.is_empty() && text.len() <= MAX_SECTION && !text.contains(char::REPLACEMENT_CHARACTER)
}

/// `## <title> (<task>, <YYYY-MM-DD>)`, the text, and a line break.
pub fn block(p: &MemoryProposal, at: u64) -> String {
    let (y, m, d) = crate::utc::date(at);
    let title = crate::items::produce::one_line(&p.title, 200);
    format!(
        "## {title} ({}, {y:04}-{m:02}-{d:02})\n\n{}\n",
        p.task,
        p.text.trim_end()
    )
}

/// Whether the live text has a heading of `task` already.
pub fn appended(text: &str, task: &str) -> bool {
    let mark = format!(" ({task}, ");
    text.lines()
        .any(|l| l.starts_with("## ") && l.ends_with(')') && l.contains(&mark))
}

/// Blocking. Appends `p` to the memory of the workspace at `root` once, under
/// the OS lock that the daemon and every bridge share.
pub fn append(
    config: &Config,
    root: &Path,
    p: &MemoryProposal,
    at: u64,
) -> Result<Appended, AppendError> {
    let state = config
        .state_dir
        .as_ref()
        .ok_or_else(|| AppendError::Lock("the host keeps no state directory".into()))?;
    let dir = state.join(LOCKS);
    DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(&dir)
        .map_err(|e| AppendError::Lock(format!("{}: {e}", dir.display())))?;
    let id = notes::base_of(root.as_os_str().as_encoded_bytes());
    let _lock = Lock::acquire(&dir, &id[..16], true).map_err(|e| match e {
        crate::records::StoreError::Busy(_) => AppendError::Busy,
        e => AppendError::Lock(e.to_string()),
    })?;
    let block = block(p, at);
    Ok(notes::append(root, &block, |live| appended(live, &p.task))?)
}

#[cfg(test)]
mod tests;
