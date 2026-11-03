//! A workspace's repos (ADR 0031): the git repos under its root, and those its
//! `repos` lists. The scan never follows a symlink and stops at its limits.

use std::ffi::OsStr;
use std::path::{Component, Path, PathBuf};
use std::time::{Duration, Instant};

use super::workspaces::{ROUTINES_DIR, TASKS_DIR, Workspace};

/// Levels below the root: `app` and `group/app`.
const MAX_DEPTH: usize = 2;
/// Directory entries one scan reads.
const MAX_ENTRIES: usize = 2000;
const MAX_SCAN: Duration = Duration::from_millis(250);

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum RepoError {
    #[error("the repo is empty; give its name under the workspace root, or its absolute path")]
    Empty,
    #[error("no repo {name:?} under {root} or in the workspace's repos")]
    NotFound { name: String, root: PathBuf },
    #[error("{name:?} leads outside the workspace root {root}")]
    Outside { name: String, root: PathBuf },
}

impl Workspace {
    /// The repos under the root and the listed ones, sorted, each once.
    /// Blocking: it scans the root.
    pub fn repo_list(&self) -> Vec<PathBuf> {
        let mut all = discover(&self.root);
        all.extend(self.repos.iter().cloned());
        all.sort();
        all.dedup();
        all
    }

    /// Whether `main`, a canonical main checkout, is one of this workspace's
    /// repos. Does not scan.
    pub fn has_repo(&self, main: &Path) -> bool {
        self.repos.iter().any(|r| r == main) || under_root(&self.root, main)
    }

    /// `raw` as a path. An absolute path stays as given; the caller checks it
    /// is a repo. A relative one is resolved under the root and must stay
    /// inside it after symlinks, unless it reaches a listed repo. A plain name
    /// that is not under the root may name a listed repo by its folder name.
    pub fn repo_path(&self, raw: &str) -> Result<PathBuf, RepoError> {
        if raw.trim().is_empty() {
            return Err(RepoError::Empty);
        }
        let given = Path::new(raw);
        if given.is_absolute() {
            return Ok(given.to_owned());
        }
        match self.root.join(given).canonicalize() {
            Ok(path) if path.starts_with(&self.root) || self.repos.contains(&path) => Ok(path),
            Ok(_) => Err(RepoError::Outside {
                name: raw.to_owned(),
                root: self.root.clone(),
            }),
            Err(_) => self.listed_by_name(raw).ok_or_else(|| RepoError::NotFound {
                name: raw.to_owned(),
                root: self.root.clone(),
            }),
        }
    }

    /// The one listed repo whose folder is named `name`.
    fn listed_by_name(&self, name: &str) -> Option<PathBuf> {
        let mut matches = self
            .repos
            .iter()
            .filter(|r| r.file_name() == Some(OsStr::new(name)));
        match (matches.next(), matches.next()) {
            (Some(one), None) => Some(one.clone()),
            _ => None,
        }
    }
}

/// The git repos under `root`, sorted: directories with a `.git` directory
/// (a worktree's `.git` file does not count), at most two levels down. Skips
/// symlinks, dot-directories, `tasks/` and `routines/`, and the inside of a repo.
pub fn discover(root: &Path) -> Vec<PathBuf> {
    let mut budget = Budget {
        entries: MAX_ENTRIES,
        until: Instant::now() + MAX_SCAN,
        spent: false,
    };
    let mut found = Vec::new();
    scan(root, 1, &mut budget, &mut found);
    if budget.spent {
        tracing::warn!(root = %root.display(), repos = found.len(), "the repo scan stopped at its limit");
    }
    found.sort();
    found
}

/// Whether `discover(root)` would find `main`, without scanning.
pub fn under_root(root: &Path, main: &Path) -> bool {
    let Ok(rel) = main.strip_prefix(root) else {
        return false;
    };
    let mut names = Vec::new();
    for component in rel.components() {
        match component {
            Component::Normal(name) => names.push(name),
            _ => return false,
        }
    }
    if names.is_empty() || names.len() > MAX_DEPTH {
        return false;
    }
    let mut dir = root.to_path_buf();
    for (i, name) in names.iter().enumerate() {
        if !candidate(name, i + 1) {
            return false;
        }
        dir.push(name);
        let last = i + 1 == names.len();
        if !dir.symlink_metadata().is_ok_and(|m| m.is_dir()) || is_repo(&dir) != last {
            return false;
        }
    }
    true
}

struct Budget {
    entries: usize,
    until: Instant,
    spent: bool,
}

impl Budget {
    fn take(&mut self) -> bool {
        if self.entries == 0 || Instant::now() >= self.until {
            self.spent = true;
            return false;
        }
        self.entries -= 1;
        true
    }
}

fn scan(dir: &Path, depth: usize, budget: &mut Budget, found: &mut Vec<PathBuf>) {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(e) => {
            tracing::debug!(dir = %dir.display(), error = %e, "cannot read a folder for repos");
            return;
        }
    };
    for entry in entries {
        if !budget.take() {
            return;
        }
        let Ok(entry) = entry else { continue };
        // `file_type` does not follow a symlink, so none is entered.
        if !entry.file_type().is_ok_and(|t| t.is_dir()) || !candidate(&entry.file_name(), depth) {
            continue;
        }
        let path = entry.path();
        if is_repo(&path) {
            found.push(path);
        } else if depth < MAX_DEPTH {
            scan(&path, depth + 1, budget, found);
        }
    }
}

/// A folder the scan looks at. Names that are not UTF-8 are skipped.
fn candidate(name: &OsStr, depth: usize) -> bool {
    let Some(name) = name.to_str() else {
        return false;
    };
    !name.starts_with('.') && !(depth == 1 && (name == TASKS_DIR || name == ROUTINES_DIR))
}

fn is_repo(dir: &Path) -> bool {
    dir.join(".git")
        .symlink_metadata()
        .is_ok_and(|m| m.is_dir())
}

#[cfg(test)]
mod tests;
