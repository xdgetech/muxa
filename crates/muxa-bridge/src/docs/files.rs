//! Which Markdown (and, when asked, HTML) files a pane may read, and reading them. A client names a
//! document only by a path from the list built here; nothing else is served.
//! Everything here blocks (git, the file system); run it on the blocking pool.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::UNIX_EPOCH;

use muxa_protocol::{DocInfo, DocKind};

use crate::loops::check::REPORT as CHECK;

/// The Task's brief and handoff, which git does not list (ADR 0015, 0018).
const BRIEF: &str = ".muxa/TASK.md";
const HANDOFF: &str = ".muxa/HANDOFF.md";

/// Documents listed for one pane at most.
pub const MAX_DOCS: usize = 2000;
/// A larger document is refused.
pub const MAX_DOC: u64 = 2 << 20;
/// Bytes read from the start of an HTML file to find its `<title>`.
const TITLE_HEAD: u64 = 8 << 10;

/// A listed document and where it really is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Doc {
    pub info: DocInfo,
    pub file: PathBuf,
    /// The file must stay inside this directory, even through symlinks.
    pub root: PathBuf,
    /// git ignores the file: it is not in the repo.
    pub ignored: bool,
}

#[derive(Debug, Default)]
pub struct Listing {
    pub root: Option<String>,
    pub docs: Vec<Doc>,
    pub truncated: bool,
}

/// A Task's or a Routine run's folder (protocol v2): its repo's documents,
/// or outside a repo every document in the folder. No agent plans: they
/// are found by pane.
pub fn for_folder(dir: &Path, html: bool) -> Listing {
    let keep = if html { is_doc_or_html } else { is_doc };
    let base = dir.canonicalize().unwrap_or_else(|_| dir.to_path_buf());
    build(Listing::default(), &base, html, || {
        super::folder::scan(&base, keep, None)
    })
}

/// Adds the repo's documents of `path`, or `outside()` when it is in no
/// repo, then the Task files.
fn build(
    mut listing: Listing,
    path: &Path,
    html: bool,
    outside: impl FnOnce() -> Vec<super::folder::Found>,
) -> Listing {
    let root = repo_root(path);
    if let Some(root) = &root {
        listing.root = root.file_name().map(|n| n.to_string_lossy().into_owned());
        let (files, truncated) = repo_files(root, html, MAX_DOCS);
        listing.truncated = truncated;
        listing.docs.extend(files.into_iter().map(|(rel, ignored)| {
            let file = root.join(&rel);
            Doc {
                info: DocInfo {
                    title: html_title(&rel, &file, root),
                    path: rel,
                    kind: DocKind::Repo,
                    mtime: mtime(&file),
                },
                file,
                root: root.clone(),
                ignored,
            }
        }));
    }
    let in_repo = root.is_some();
    let base = root.unwrap_or_else(|| path.to_path_buf());
    if !in_repo {
        let mut found = outside();
        found.sort_by(|a, b| a.path.cmp(&b.path));
        listing.truncated = found.len() > MAX_DOCS;
        found.truncate(MAX_DOCS);
        listing.docs.extend(found.into_iter().map(|f| {
            let file = base.join(&f.path);
            Doc {
                info: DocInfo {
                    title: html_title(&f.path, &file, &base),
                    path: f.path,
                    kind: DocKind::Repo,
                    mtime: f.mtime,
                },
                file,
                root: base.clone(),
                ignored: false,
            }
        }));
    }
    for path in [BRIEF, HANDOFF, CHECK] {
        if let Some(doc) = task_file(&base, path)
            && listing.find(path).is_err()
        {
            listing.docs.push(doc);
        }
    }
    listing
}

/// A task's `TASK.md`, `HANDOFF.md` or `CHECK.md` (ADR 0015, 0018, 0019), at the repo root or in
/// the pane's directory. git ignores `.muxa/`, and a task without a repo has no
/// repo to list from.
fn task_file(base: &Path, path: &str) -> Option<Doc> {
    let file = base.join(path);
    if !file.is_file() {
        return None;
    }
    Some(Doc {
        info: DocInfo {
            path: path.to_owned(),
            kind: DocKind::Repo,
            title: title(&file),
            mtime: mtime(&file),
        },
        file,
        root: base.to_path_buf(),
        ignored: false,
    })
}

pub(crate) fn repo_root(path: &Path) -> Option<PathBuf> {
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

/// The Markdown files a pane lists, as git pathspecs.
const DOC_PATTERNS: [&str; 2] = ["*.md", "*.markdown"];
/// The HTML files a pane lists when the client asks, as git pathspecs.
const HTML_PATTERNS: [&str; 2] = ["*.html", "*.htm"];

fn matches(patterns: &[&str], path: &str) -> bool {
    patterns
        .iter()
        .any(|p| p.strip_prefix('*').is_some_and(|ext| path.ends_with(ext)))
}

/// A path the Markdown listing would take, by its name alone.
pub(crate) fn is_doc(path: &str) -> bool {
    matches(&DOC_PATTERNS, path)
}

pub(crate) fn is_html(path: &str) -> bool {
    matches(&HTML_PATTERNS, path)
}

pub(crate) fn is_doc_or_html(path: &str) -> bool {
    is_doc(path) || is_html(path)
}

/// Folders whose ignored files are never listed: dependencies and build output, which are heavy
/// and not the user's documents.
const SKIPPED_DIRS: [&str; 7] = [
    "node_modules",
    "target",
    "vendor",
    "Pods",
    "DerivedData",
    "bower_components",
    "venv",
];
/// Ignored files whose name holds one of these may carry a secret (ADR 0099).
const SECRET_WORDS: [&str; 5] = ["secret", "credential", "password", "token", "private"];

/// The repo's documents as (path, ignored), at most `max`. Tracked and not-ignored files come
/// first, so ignored output never pushes them out.
/// Ignored files are listed too (ADR 0099), except under a dot name or a skipped folder, or
/// with a secret-like name. Only Markdown (and HTML when asked) is ever listed.
fn repo_files(root: &Path, html: bool, max: usize) -> (Vec<(String, bool)>, bool) {
    let html_patterns: &[&str] = if html { &HTML_PATTERNS } else { &[] };
    let patterns: Vec<String> = DOC_PATTERNS
        .iter()
        .chain(html_patterns)
        .map(|p| (*p).to_owned())
        .collect();
    let mut files = ls_files(
        root,
        &["--cached", "--others", "--exclude-standard"],
        &patterns,
    );
    let skipped: Vec<String> = SKIPPED_DIRS
        .iter()
        .map(|d| format!(":(exclude,glob)**/{d}/**"))
        .collect();
    let mut ignored: Vec<String> = ls_files(
        root,
        &["--others", "--ignored", "--exclude-standard"],
        &[patterns, skipped].concat(),
    )
    .into_iter()
    .filter(|p| keep_ignored(p))
    .collect();
    let truncated = files.len() + ignored.len() > max;
    files.truncate(max);
    ignored.truncate(max - files.len());
    let mut out: Vec<(String, bool)> = files
        .into_iter()
        .map(|p| (p, false))
        .chain(ignored.into_iter().map(|p| (p, true)))
        .collect();
    out.sort();
    (out, truncated)
}

/// The sorted paths `git ls-files -z` prints for `flags` and `pathspecs`; none when git fails.
fn ls_files(root: &Path, flags: &[&str], pathspecs: &[String]) -> Vec<String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["ls-files", "-z"])
        .args(flags)
        .arg("--")
        .args(pathspecs)
        .output();
    let out = match out {
        Ok(out) if out.status.success() => out,
        Ok(out) => {
            tracing::warn!(status = %out.status, "git ls-files failed for documents");
            return Vec::new();
        }
        Err(e) => {
            tracing::warn!(error = %e, "cannot run git to list documents");
            return Vec::new();
        }
    };
    let mut paths: Vec<String> = out
        .stdout
        .split(|&b| b == 0)
        .filter(|p| !p.is_empty())
        .filter_map(|p| String::from_utf8(p.to_vec()).ok())
        .collect();
    paths.sort();
    paths.dedup();
    paths
}

/// An ignored file the Reader may list: no dot name and no skipped folder on its path, and no
/// secret-like word in its name.
fn keep_ignored(path: &str) -> bool {
    let mut parts = path.split('/');
    let name = parts.next_back().unwrap_or_default();
    let dirs_ok = parts.all(|d| !d.starts_with('.') && !SKIPPED_DIRS.contains(&d));
    let lower = name.to_ascii_lowercase();
    dirs_ok && !name.starts_with('.') && !SECRET_WORDS.iter().any(|w| lower.contains(w))
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

/// An HTML file's `<title>`; Markdown files are listed without a title.
/// None for a file outside `root`, as `read_bytes` would refuse it.
fn html_title(path: &str, file: &Path, root: &Path) -> Option<String> {
    if !is_html(path) {
        return None;
    }
    let real = file.canonicalize().ok()?;
    if !real.starts_with(root.canonicalize().ok()?) {
        return None;
    }
    // Only the head: a listing may hold many large reports.
    let mut bytes = Vec::new();
    std::fs::File::open(&real)
        .ok()?
        .take(TITLE_HEAD)
        .read_to_end(&mut bytes)
        .ok()?;
    let head = String::from_utf8_lossy(&bytes).into_owned();
    let lower = head.to_ascii_lowercase();
    let open = lower.find("<title")?;
    let start = open + lower[open..].find('>')? + 1;
    let end = start + lower[start..].find("</title")?;
    Some(
        head[start..end]
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" "),
    )
    .filter(|t| !t.is_empty())
}

pub(crate) fn mtime(path: &Path) -> Option<u64> {
    let modified = std::fs::metadata(path).ok()?.modified().ok()?;
    Some(modified.duration_since(UNIX_EPOCH).ok()?.as_secs())
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ReadError {
    #[error("{0} is not a listed file")]
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
