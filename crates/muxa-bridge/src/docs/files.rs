//! Which Markdown files a pane may read, and reading them. A client names a
//! document only by a path from the list built here; nothing else is served.
//! Everything here blocks (git, the file system); run it on the blocking pool.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::UNIX_EPOCH;

use muxa_protocol::{DocInfo, DocKind};

use crate::agents::{AgentDoc, PaneInfo, Registry};

/// Documents listed for one pane at most.
pub const MAX_DOCS: usize = 2000;
/// A larger document is refused.
pub const MAX_DOC: u64 = 2 << 20;
const PLAN_PREFIX: &str = "plans/";

/// A listed document and where it really is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Doc {
    pub info: DocInfo,
    pub file: PathBuf,
    /// The file must stay inside this directory, even through symlinks.
    pub root: PathBuf,
}

#[derive(Debug, Default)]
pub struct Listing {
    pub root: Option<String>,
    pub docs: Vec<Doc>,
    pub truncated: bool,
}

/// The agent's documents first (a plan), then the repo's Markdown files in path order.
pub fn list(pane: &PaneInfo, agents: &Registry) -> Listing {
    let mut listing = Listing::default();
    match agents.documents(pane) {
        Ok(docs) => listing.docs.extend(docs.into_iter().map(agent_doc)),
        // Without its plan the list is still useful; the reason goes to the log.
        Err(e) => {
            tracing::warn!(pane = %pane.pane, error = %e, "cannot list the agent's documents")
        }
    }
    if let Some(root) = repo_root(Path::new(&pane.path)) {
        listing.root = root.file_name().map(|n| n.to_string_lossy().into_owned());
        let (files, truncated) = repo_files(&root);
        listing.truncated = truncated;
        listing.docs.extend(files.into_iter().map(|rel| {
            let file = root.join(&rel);
            Doc {
                info: DocInfo {
                    path: rel,
                    kind: DocKind::Repo,
                    title: None,
                    mtime: mtime(&file),
                },
                file,
                root: root.clone(),
            }
        }));
    }
    listing
}

fn agent_doc(d: AgentDoc) -> Doc {
    Doc {
        info: DocInfo {
            path: format!("{PLAN_PREFIX}{}", d.name),
            kind: DocKind::Plan,
            title: title(&d.path),
            mtime: mtime(&d.path),
        },
        file: d.path,
        root: d.root,
    }
}

pub(super) fn repo_root(path: &Path) -> Option<PathBuf> {
    let out = Command::new("git")
        .arg("-C")
        .arg(path)
        .args(["rev-parse", "--show-toplevel"])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8(out.stdout).ok()?;
    Some(PathBuf::from(text.trim_end_matches('\n')))
}

/// Tracked files, and untracked ones git does not ignore: a plan an agent just
/// wrote shows up, a secret in an ignored file does not.
fn repo_files(root: &Path) -> (Vec<String>, bool) {
    let out = Command::new("git")
        .arg("-C")
        .arg(root)
        .args([
            "ls-files",
            "-z",
            "--cached",
            "--others",
            "--exclude-standard",
            "--",
            "*.md",
            "*.markdown",
        ])
        .output();
    let Ok(out) = out else {
        return (Vec::new(), false);
    };
    if !out.status.success() {
        return (Vec::new(), false);
    }
    let mut files: Vec<String> = out
        .stdout
        .split(|&b| b == 0)
        .filter(|p| !p.is_empty())
        .filter_map(|p| String::from_utf8(p.to_vec()).ok())
        .collect();
    files.sort();
    files.dedup();
    let truncated = files.len() > MAX_DOCS;
    files.truncate(MAX_DOCS);
    (files, truncated)
}

/// The first `# ` heading, from the start of the file.
fn title(path: &Path) -> Option<String> {
    let text = std::fs::read(path).ok()?;
    let head = String::from_utf8_lossy(&text[..text.len().min(8 << 10)]).into_owned();
    head.lines()
        .find_map(|l| l.strip_prefix("# "))
        .map(|t| t.trim().to_owned())
        .filter(|t| !t.is_empty())
}

pub(super) fn mtime(path: &Path) -> Option<u64> {
    let modified = std::fs::metadata(path).ok()?.modified().ok()?;
    Some(modified.duration_since(UNIX_EPOCH).ok()?.as_secs())
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ReadError {
    #[error("{0} is not one of this pane's documents")]
    NotListed(String),
    #[error("{0} is outside its directory")]
    Outside(String),
    #[error("{0} is larger than {1} bytes")]
    TooLarge(String, u64),
    #[error("{path}: {reason}")]
    Io { path: String, reason: String },
}

impl Listing {
    pub fn find(&self, path: &str) -> Result<&Doc, ReadError> {
        self.docs
            .iter()
            .find(|d| d.info.path == path)
            .ok_or_else(|| ReadError::NotListed(path.to_owned()))
    }
}

/// The text of `doc` and its mtime.
pub fn read(doc: &Doc) -> Result<(String, Option<u64>), ReadError> {
    let (bytes, mtime) = read_bytes(&doc.file, &doc.root, &doc.info.path, MAX_DOC)?;
    Ok((String::from_utf8_lossy(&bytes).into_owned(), mtime))
}

/// The bytes of the listed file `file`, named `path` in errors, and its mtime.
/// Checked again on every read: a symlink can be changed after the file was listed.
pub(super) fn read_bytes(
    file: &Path,
    root: &Path,
    path: &str,
    max: u64,
) -> Result<(Vec<u8>, Option<u64>), ReadError> {
    let io = |e: std::io::Error| ReadError::Io {
        path: path.to_owned(),
        reason: e.to_string(),
    };
    let real = file.canonicalize().map_err(io)?;
    let root = root.canonicalize().map_err(io)?;
    if !real.starts_with(&root) {
        return Err(ReadError::Outside(path.to_owned()));
    }
    let meta = std::fs::metadata(&real).map_err(io)?;
    if !meta.is_file() {
        return Err(ReadError::NotListed(path.to_owned()));
    }
    if meta.len() > max {
        return Err(ReadError::TooLarge(path.to_owned(), max));
    }
    let bytes = std::fs::read(&real).map_err(io)?;
    Ok((bytes, mtime(&real)))
}

#[cfg(test)]
mod tests;
