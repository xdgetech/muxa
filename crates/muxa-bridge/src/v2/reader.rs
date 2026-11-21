//! The Reader on protocol v2 (plan F2): a Task's or a Routine run's folder
//! served through `docs` and `changes`, as v1 serves a pane's. A client
//! names a file only by a path these lists hold; each read lists again.
//! File content never reaches the log.

use std::path::Path;

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use muxa_protocol::v2::{
    DiffScope, ReaderBranch, ReaderContent, ReaderEntry, ReaderKind, ReaderList,
};
use muxa_protocol::{ChangeInfo, DocKind};
use muxa_server::{ReaderOpen, TaskError};

use crate::changes::git::{self, DiffError};
use crate::docs::{self, files::ReadError};

/// The largest image sent: its base64 fits in one 16 MiB frame.
const MAX_IMAGE: u64 = 11 << 20;

pub async fn list(dir: &Path, kind: ReaderKind, html: bool) -> Result<ReaderList, TaskError> {
    let dir = dir.to_path_buf();
    blocking(move || match kind {
        ReaderKind::Docs => Ok(doc_list(&dir, html)),
        ReaderKind::Images => Ok(image_list(&dir)),
        ReaderKind::Changes => change_list(&dir),
    })
    .await
}

pub async fn read(dir: &Path, open: &ReaderOpen) -> Result<ReaderContent, TaskError> {
    let (dir, open) = (dir.to_path_buf(), open.clone());
    blocking(move || match open.kind {
        ReaderKind::Docs => read_doc(&dir, &open),
        ReaderKind::Images => read_image(&dir, &open.path),
        ReaderKind::Changes => read_diff(&dir, &open),
    })
    .await
}

async fn blocking<T: Send + 'static>(
    f: impl FnOnce() -> Result<T, TaskError> + Send + 'static,
) -> Result<T, TaskError> {
    tokio::task::spawn_blocking(f)
        .await
        .map_err(|e| TaskError::Failed(format!("the Reader stopped: {e}")))?
}

fn doc_list(dir: &Path, html: bool) -> ReaderList {
    let listing = docs::files::for_folder(dir, html);
    ReaderList {
        root: listing.root,
        entries: listing
            .docs
            .into_iter()
            .filter(|d| d.info.kind == DocKind::Repo)
            .map(|d| ReaderEntry {
                path: d.info.path,
                title: d.info.title,
                mtime: d.info.mtime,
                ..ReaderEntry::default()
            })
            .collect(),
        truncated: listing.truncated,
        branch: None,
    }
}

fn read_doc(dir: &Path, open: &ReaderOpen) -> Result<ReaderContent, TaskError> {
    let listing = docs::files::for_folder(dir, open.html);
    let doc = listing.find(&open.path).map_err(read_error)?;
    let (text, mtime) = docs::files::read(doc).map_err(read_error)?;
    Ok(ReaderContent {
        path: open.path.clone(),
        text: Some(text),
        mtime,
        ..ReaderContent::default()
    })
}

fn image_list(dir: &Path) -> ReaderList {
    let listing = docs::images::for_folder(dir);
    ReaderList {
        root: listing.root,
        entries: listing
            .images
            .into_iter()
            .map(|i| ReaderEntry {
                path: i.info.path,
                mtime: i.info.mtime,
                size: i.info.size,
                ..ReaderEntry::default()
            })
            .collect(),
        truncated: listing.truncated,
        branch: None,
    }
}

fn read_image(dir: &Path, path: &str) -> Result<ReaderContent, TaskError> {
    let listing = docs::images::for_folder(dir);
    let image = listing.find(path).map_err(read_error)?;
    if image.info.size.is_some_and(|n| n > MAX_IMAGE) {
        return Err(read_error(ReadError::TooLarge(path.to_owned(), MAX_IMAGE)));
    }
    let (bytes, mtime) = docs::images::read(image).map_err(read_error)?;
    if bytes.len() as u64 > MAX_IMAGE {
        return Err(read_error(ReadError::TooLarge(path.to_owned(), MAX_IMAGE)));
    }
    Ok(ReaderContent {
        path: path.to_owned(),
        data: Some(STANDARD.encode(bytes)),
        mtime,
        ..ReaderContent::default()
    })
}

fn change_list(dir: &Path) -> Result<ReaderList, TaskError> {
    let changes = git::list(dir);
    if changes.root.is_none() {
        return Err(TaskError::Invalid(
            "this folder is not in a git repo, so it has no changes".into(),
        ));
    }
    Ok(ReaderList {
        root: changes.root_name(),
        entries: changes.entries.into_iter().map(change_entry).collect(),
        truncated: changes.truncated,
        branch: changes.branch.map(|b| ReaderBranch {
            base: b.base,
            commits: b.commits,
            entries: b.entries.into_iter().map(change_entry).collect(),
            truncated: b.truncated,
        }),
    })
}

fn change_entry(c: ChangeInfo) -> ReaderEntry {
    ReaderEntry {
        path: c.path,
        status: Some(c.status),
        old_path: c.old_path,
        added: c.added,
        removed: c.removed,
        ..ReaderEntry::default()
    }
}

fn read_diff(dir: &Path, open: &ReaderOpen) -> Result<ReaderContent, TaskError> {
    let changes = git::list(dir);
    let branch = open.scope == DiffScope::Branch;
    let (Some((change, scope)), Some(root)) =
        (changes.find(&open.path, branch), changes.root.as_ref())
    else {
        return Err(TaskError::Invalid(format!(
            "{} is not one of this folder's changed files",
            open.path
        )));
    };
    let diff = git::diff(root, change, &scope).map_err(|e| {
        if !matches!(e, DiffError::Unchanged(_)) {
            tracing::warn!(error = %e, "a v2 diff failed");
        }
        TaskError::Invalid(e.to_string())
    })?;
    Ok(ReaderContent {
        path: open.path.clone(),
        binary: diff.text.is_none(),
        text: diff.text,
        ..ReaderContent::default()
    })
}

fn read_error(e: ReadError) -> TaskError {
    match e {
        ReadError::Io { .. } => {
            tracing::warn!(error = %e, "a v2 Reader file cannot be read");
            TaskError::Failed(e.to_string())
        }
        _ => TaskError::Invalid(e.to_string()),
    }
}

#[cfg(test)]
#[path = "reader_tests.rs"]
mod tests;
