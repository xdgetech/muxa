//! What differs from HEAD in a repo, what its branch changed since it left its
//! base, and one file's diff. A client names a file only by a path from the
//! lists built here. Everything here blocks; run it on the blocking pool.

use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use muxa_protocol::{ChangeInfo, ChangeStatus};

/// Files listed for one pane at most.
pub const MAX_CHANGES: usize = 2000;
/// A longer diff is refused.
pub const MAX_DIFF: usize = 2 << 20;

#[derive(Debug, Default)]
pub struct Changes {
    pub root: Option<PathBuf>,
    /// Against HEAD: staged, unstaged and untracked.
    pub entries: Vec<ChangeInfo>,
    pub truncated: bool,
    /// What the branch's commits changed; None on the base itself or without one.
    pub branch: Option<Branch>,
}

/// The files the commits since the fork point changed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Branch {
    /// The ref compared with, e.g. `origin/main`.
    pub base: String,
    /// `git merge-base HEAD <base>`.
    pub fork: String,
    pub commits: u32,
    pub entries: Vec<ChangeInfo>,
    pub truncated: bool,
}

/// Which diff of a file: against HEAD, or the branch's commits since `fork`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Scope {
    Uncommitted,
    Branch { fork: String },
}

impl Changes {
    /// A listed file of `scope`, and the scope with what its diff needs.
    pub fn find(&self, path: &str, branch: bool) -> Option<(&ChangeInfo, Scope)> {
        if !branch {
            let change = self.entries.iter().find(|c| c.path == path)?;
            return Some((change, Scope::Uncommitted));
        }
        let b = self.branch.as_ref()?;
        let change = b.entries.iter().find(|c| c.path == path)?;
        Some((
            change,
            Scope::Branch {
                fork: b.fork.clone(),
            },
        ))
    }

    pub fn root_name(&self) -> Option<String> {
        let name = self.root.as_ref()?.file_name()?;
        Some(name.to_string_lossy().into_owned())
    }
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum DiffError {
    #[error("{0} has no changes now")]
    Unchanged(String),
    #[error("the diff of {0} is larger than {MAX_DIFF} bytes")]
    TooLarge(String),
    #[error("git diff failed for {path}: {reason}")]
    Git { path: String, reason: String },
}

/// One file's diff: the text, or None for a binary file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diff {
    pub text: Option<String>,
}

/// git in `dir`. It never takes optional locks, so it does not contend with
/// the agent's own git commands.
fn git(dir: &Path) -> Command {
    let mut cmd = Command::new("git");
    cmd.arg("-C")
        .arg(dir)
        .env("GIT_OPTIONAL_LOCKS", "0")
        .stdin(Stdio::null())
        .stderr(Stdio::piped());
    cmd
}

/// The changed files of the repo that holds `dir`; empty outside a repo.
pub fn list(dir: &Path) -> Changes {
    let Some(root) = toplevel(dir) else {
        return Changes::default();
    };
    let mut entries = status(&root);
    let counts = numstat(&root);
    for e in &mut entries {
        if let Some(&(added, removed)) = counts.get(&e.path) {
            e.added = added;
            e.removed = removed;
        } else if e.status == ChangeStatus::Untracked {
            (e.added, e.removed) = untracked_lines(&root.join(&e.path));
        }
    }
    entries.sort_by(|a, b| a.path.cmp(&b.path));
    let truncated = entries.len() > MAX_CHANGES;
    entries.truncate(MAX_CHANGES);
    Changes {
        branch: branch(&root),
        root: Some(root),
        entries,
        truncated,
    }
}

/// The branch's own commits against its base. None on the base itself, when
/// the repo has no base, or when HEAD and the base share no history.
fn branch(root: &Path) -> Option<Branch> {
    let base = default_branch(root)?;
    let fork = output_line(git(root).args(["merge-base", "HEAD", &base]))?;
    let head = output_line(git(root).args(["rev-parse", "HEAD"]))?;
    if fork == head {
        return None;
    }
    let range = format!("{fork}..HEAD");
    let commits = output_line(git(root).args(["rev-list", "--count", &range]))?
        .parse()
        .ok()?;
    let out = git(root)
        .args(["diff", "--name-status", "-z", "-M", "--no-ext-diff"])
        .args([&fork, "HEAD"])
        .output()
        .ok()
        .filter(|o| o.status.success())?;
    let mut entries = parse_name_status(&out.stdout);
    let counts = git(root)
        .args([
            "diff",
            "--numstat",
            "-z",
            "-M",
            "--no-ext-diff",
            "--no-textconv",
        ])
        .args([&fork, "HEAD"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| parse_numstat(&o.stdout))
        .unwrap_or_default();
    for e in &mut entries {
        if let Some(&(added, removed)) = counts.get(&e.path) {
            e.added = added;
            e.removed = removed;
        }
    }
    entries.sort_by(|a, b| a.path.cmp(&b.path));
    let truncated = entries.len() > MAX_CHANGES;
    entries.truncate(MAX_CHANGES);
    Some(Branch {
        base,
        fork,
        commits,
        entries,
        truncated,
    })
}

/// origin's default branch; else the main worktree's branch, when this is
/// another worktree (a task); else a local `main` or `master`. Tasks start from
/// the first two too (`default_base` in `tasks/builtin.rs`).
fn default_branch(root: &Path) -> Option<String> {
    if let Some(origin) =
        output_line(git(root).args(["symbolic-ref", "-q", "--short", "refs/remotes/origin/HEAD"]))
    {
        return Some(origin);
    }
    let current = output_line(git(root).args(["symbolic-ref", "-q", "--short", "HEAD"]));
    if let Some(main) = main_worktree_branch(root).filter(|b| Some(b) != current.as_ref()) {
        return Some(main);
    }
    ["main", "master"]
        .into_iter()
        .find(|b| {
            Some(*b) != current.as_deref()
                && git(root)
                    .args(["show-ref", "--verify", "-q", &format!("refs/heads/{b}")])
                    .output()
                    .is_ok_and(|o| o.status.success())
        })
        .map(str::to_owned)
}

fn main_worktree_branch(root: &Path) -> Option<String> {
    let out = git(root)
        .args(["worktree", "list", "--porcelain"])
        .output()
        .ok()
        .filter(|o| o.status.success())?;
    // The main worktree is listed first; its block ends at the first blank line.
    let text = String::from_utf8_lossy(&out.stdout).into_owned();
    text.lines()
        .take_while(|l| !l.is_empty())
        .find_map(|l| l.strip_prefix("branch refs/heads/"))
        .map(str::to_owned)
}

/// The first line of a successful command's output.
fn output_line(cmd: &mut Command) -> Option<String> {
    let out = cmd.output().ok().filter(|o| o.status.success())?;
    let text = String::from_utf8(out.stdout).ok()?;
    let line = text.lines().next()?.trim();
    (!line.is_empty()).then(|| line.to_owned())
}

/// `git diff --name-status -z`: a status letter, then the path; a rename or copy
/// has a score after its letter and two paths, the old one first.
pub(crate) fn parse_name_status(bytes: &[u8]) -> Vec<ChangeInfo> {
    let mut fields = bytes
        .split(|&b| b == 0)
        .map(|f| String::from_utf8_lossy(f).into_owned());
    let mut out = Vec::new();
    while let Some(code) = fields.next() {
        let (status, old_path) = match code.as_bytes().first() {
            Some(b'R') => (ChangeStatus::Renamed, fields.next()),
            // A copy is a new file; its source stays as it was.
            Some(b'C') => {
                fields.next();
                (ChangeStatus::Added, None)
            }
            Some(b'A') => (ChangeStatus::Added, None),
            Some(b'D') => (ChangeStatus::Deleted, None),
            Some(b'U') => (ChangeStatus::Conflicted, None),
            Some(_) => (ChangeStatus::Modified, None),
            None => continue,
        };
        let Some(path) = fields.next() else { break };
        out.push(ChangeInfo {
            path,
            status,
            old_path,
            added: None,
            removed: None,
        });
    }
    out
}

fn toplevel(dir: &Path) -> Option<PathBuf> {
    let out = git(dir)
        .args(["rev-parse", "--show-toplevel"])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8(out.stdout).ok()?;
    Some(PathBuf::from(text.trim_end_matches('\n')))
}

fn status(root: &Path) -> Vec<ChangeInfo> {
    let out = git(root)
        .args(["status", "--porcelain=v2", "-z", "--untracked-files=all"])
        .output();
    match out {
        Ok(out) if out.status.success() => parse_status(&out.stdout),
        Ok(out) => {
            tracing::warn!(status = %out.status, "git status failed");
            Vec::new()
        }
        Err(e) => {
            tracing::warn!(error = %e, "cannot run git status");
            Vec::new()
        }
    }
}

/// `git status --porcelain=v2 -z`, against HEAD: staged and unstaged as one.
pub(crate) fn parse_status(bytes: &[u8]) -> Vec<ChangeInfo> {
    let mut fields = bytes
        .split(|&b| b == 0)
        .map(|f| String::from_utf8_lossy(f).into_owned());
    let mut out = Vec::new();
    while let Some(record) = fields.next() {
        let entry = match record.as_bytes().first() {
            Some(b'1') => record
                .splitn(9, ' ')
                .nth(8)
                .map(|p| (p.to_owned(), xy(&record), None)),
            Some(b'2') => {
                let path = record.splitn(10, ' ').nth(9).map(str::to_owned);
                // A rename's old path is the next NUL-separated field.
                let old = fields.next();
                path.map(|p| (p, ChangeStatus::Renamed, old))
            }
            Some(b'u') => record
                .splitn(11, ' ')
                .nth(10)
                .map(|p| (p.to_owned(), ChangeStatus::Conflicted, None)),
            Some(b'?') => record
                .get(2..)
                .map(|p| (p.to_owned(), ChangeStatus::Untracked, None)),
            _ => None,
        };
        if let Some((path, status, old_path)) = entry {
            out.push(ChangeInfo {
                path,
                status,
                old_path,
                added: None,
                removed: None,
            });
        }
    }
    out
}

/// The change against HEAD from an ordinary entry's index and worktree letters.
fn xy(record: &str) -> ChangeStatus {
    let code = record.split(' ').nth(1).unwrap_or("..");
    if code.contains('D') {
        ChangeStatus::Deleted
    } else if code.starts_with('A') {
        ChangeStatus::Added
    } else {
        ChangeStatus::Modified
    }
}

/// Lines added and removed per path against HEAD; None for a binary file.
type Counts = HashMap<String, (Option<u32>, Option<u32>)>;

fn numstat(root: &Path) -> Counts {
    let Some(base) = base(root) else {
        return Counts::new();
    };
    let out = git(root)
        .args([
            "diff",
            "--numstat",
            "-z",
            "-M",
            "--no-ext-diff",
            "--no-textconv",
        ])
        .arg(&base)
        .output();
    match out {
        Ok(out) if out.status.success() => parse_numstat(&out.stdout),
        _ => Counts::new(),
    }
}

/// `git diff --numstat -z`: "added\tremoved\tpath", or for a rename
/// "added\tremoved\t", the old path and the new path as separate fields.
pub(crate) fn parse_numstat(bytes: &[u8]) -> Counts {
    let mut fields = bytes
        .split(|&b| b == 0)
        .map(|f| String::from_utf8_lossy(f).into_owned());
    let mut counts = Counts::new();
    while let Some(record) = fields.next() {
        let mut parts = record.splitn(3, '\t');
        let (Some(added), Some(removed), Some(path)) = (parts.next(), parts.next(), parts.next())
        else {
            continue;
        };
        let path = if path.is_empty() {
            let _old = fields.next();
            match fields.next() {
                Some(new) => new,
                None => continue,
            }
        } else {
            path.to_owned()
        };
        counts.insert(path, (added.parse().ok(), removed.parse().ok()));
    }
    counts
}

/// An untracked text file is all additions.
fn untracked_lines(file: &Path) -> (Option<u32>, Option<u32>) {
    let Ok(meta) = std::fs::metadata(file) else {
        return (None, None);
    };
    if !meta.is_file() || meta.len() > MAX_DIFF as u64 {
        return (None, None);
    }
    match std::fs::read(file) {
        Ok(bytes) if !bytes.contains(&0) => {
            let lines = bytes
                .split(|&b| b == b'\n')
                .filter(|l| !l.is_empty())
                .count();
            (Some(u32::try_from(lines).unwrap_or(u32::MAX)), Some(0))
        }
        _ => (None, None),
    }
}

/// What to diff against: HEAD, or the empty tree before the first commit.
fn base(root: &Path) -> Option<String> {
    let head = git(root)
        .args(["rev-parse", "-q", "--verify", "HEAD"])
        .output()
        .ok()?;
    if head.status.success() {
        return Some("HEAD".into());
    }
    let empty = git(root)
        .args(["hash-object", "-t", "tree", "/dev/null"])
        .output()
        .ok()?;
    let id = String::from_utf8(empty.stdout).ok()?;
    Some(id.trim().to_owned()).filter(|s| !s.is_empty())
}

/// The unified diff of one listed file, in `scope`.
pub fn diff(root: &Path, change: &ChangeInfo, scope: &Scope) -> Result<Diff, DiffError> {
    let path = change.path.clone();
    let fail = |reason: String| DiffError::Git {
        path: path.clone(),
        reason,
    };
    let mut cmd = git(root);
    cmd.args(["diff", "--no-ext-diff", "--no-textconv", "--no-color", "-M"]);
    if let Scope::Branch { fork } = scope {
        cmd.args([fork.as_str(), "HEAD", "--"]);
        if let Some(old) = &change.old_path {
            cmd.arg(old);
        }
        cmd.arg(&change.path);
    } else if change.status == ChangeStatus::Untracked {
        cmd.args(["--no-index", "--", "/dev/null", &change.path]);
    } else {
        let base = base(root).ok_or_else(|| fail("no commit to compare with".into()))?;
        cmd.arg(base).arg("--");
        if let Some(old) = &change.old_path {
            cmd.arg(old);
        }
        cmd.arg(&change.path);
    }
    let (bytes, too_long, ok) = run_capped(&mut cmd, MAX_DIFF).map_err(fail)?;
    if !ok {
        return Err(fail("git exited with an error".into()));
    }
    if too_long {
        return Err(DiffError::TooLarge(path));
    }
    if bytes.is_empty() {
        return Err(DiffError::Unchanged(path));
    }
    let text = String::from_utf8_lossy(&bytes).into_owned();
    let binary = !text.contains("\n@@ ") && text.contains("Binary files ");
    Ok(Diff {
        text: (!binary).then_some(text),
    })
}

/// Reads at most `max` bytes of stdout; a longer output kills the command.
/// Returns the output, whether it was cut, and whether git succeeded. With
/// `--no-index`, exit status 1 means "differs", which is success here.
fn run_capped(cmd: &mut Command, max: usize) -> Result<(Vec<u8>, bool, bool), String> {
    let mut child = cmd
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| e.to_string())?;
    let mut bytes = Vec::new();
    if let Some(stdout) = child.stdout.take() {
        stdout
            .take(max as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(|e| e.to_string())?;
    }
    let too_long = bytes.len() > max;
    if too_long {
        let _ = child.kill();
    }
    let status = child.wait().map_err(|e| e.to_string())?;
    let ok = too_long || matches!(status.code(), Some(0 | 1));
    Ok((bytes, too_long, ok))
}

#[cfg(test)]
mod tests;
