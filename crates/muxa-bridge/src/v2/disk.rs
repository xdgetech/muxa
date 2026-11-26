//! `disk` and `disk-clean` (ADR 0102 §5): Task places, other worktrees of
//! the Workspaces' repos, Task folders and build folders, with their sizes.
//! `disk-clean` removes only rows of a fresh scan, never a held one.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use muxa_protocol::v2::{DiskKept, DiskKind, DiskRow, PlaceKind, WorktreeMode};
use muxa_server::holds::Holds;
use muxa_server::{DiskClean, DiskCleaned, TaskError};

use super::close::{self, Tree};
use super::place_records::{self, PlaceRecord};
use crate::config::Config;
use crate::records::Workspaces;

/// A worktree muxa did not make is cleanable only when its index has not
/// changed for this long: a Claude Code subagent may be using it now.
const IDLE: Duration = Duration::from_secs(86_400);
/// As the sweep's grace: a Task that is starting has no terminal yet.
const GRACE: u64 = 600;
const CLAUDE_WORKTREES: &str = ".claude/worktrees";

/// One row and what removing it needs.
struct Found {
    row: DiskRow,
    tree: Option<Tree>,
    build: Vec<PathBuf>,
    /// Why it is not cleanable without `force`; None when it is.
    why: Option<String>,
}

/// The rows, with sizes.
pub async fn report(config: &Config, holds: &Holds) -> Result<Vec<DiskRow>, TaskError> {
    let mut found = scan(config, holds).await?;
    sizes(&mut found).await;
    Ok(found.into_iter().map(|f| f.row).collect())
}

/// Removes the rows `clean` names, or all cleanable rows.
pub async fn clean(
    config: &Config,
    holds: &Holds,
    clean: DiskClean,
) -> Result<DiskCleaned, TaskError> {
    let found = scan(config, holds).await?;
    let unknown: Vec<&String> = clean
        .paths
        .iter()
        .filter(|p| !found.iter().any(|f| &f.row.path == *p))
        .collect();
    if !unknown.is_empty() {
        return Err(TaskError::Invalid(format!(
            "not in the disk list: {}. Run disk first and name its rows",
            unknown
                .iter()
                .map(|p| p.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        )));
    }
    let chosen: Vec<Found> = found
        .into_iter()
        .filter(|f| {
            if clean.paths.is_empty() {
                f.row.cleanable
            } else {
                clean.paths.contains(&f.row.path)
            }
        })
        .collect();
    let mut done = DiskCleaned::default();
    for f in chosen {
        let path = f.row.path.clone();
        let kept = |why: String| DiskKept {
            path: path.clone(),
            why,
        };
        if f.row.held.is_some() {
            done.kept.push(kept("a terminal still works there".into()));
            continue;
        }
        if let (Some(why), false) = (&f.why, clean.force) {
            done.kept
                .push(kept(format!("{why}; name it with force to remove it")));
            continue;
        }
        if clean.dry_run {
            done.removed.push(path);
            continue;
        }
        match remove(&f, clean.force).await {
            Ok(()) => {
                forget(config, Path::new(&path), f.build).await;
                done.removed.push(path);
            }
            Err(e) => done.kept.push(kept(e)),
        }
    }
    Ok(done)
}

async fn remove(f: &Found, force: bool) -> Result<(), String> {
    match f.row.kind {
        DiskKind::TaskWorktree | DiskKind::Worktree | DiskKind::ClaudeWorktree => {
            let Some(tree) = &f.tree else {
                return Err("not on a branch; remove it with git worktree remove".into());
            };
            let mode = if force {
                WorktreeMode::Force
            } else {
                WorktreeMode::Remove
            };
            close::remove_tree(tree, mode).await
        }
        DiskKind::TaskFolder | DiskKind::BuildFolder => {
            let path = PathBuf::from(&f.row.path);
            blocking(move || {
                std::fs::remove_dir_all(&path)
                    .map_err(|e| format!("cannot remove {}: {e}", path.display()))
            })
            .await
        }
    }
}

/// Drops the record of a removed place and its build folders.
async fn forget(config: &Config, path: &Path, build: Vec<PathBuf>) {
    let (state, path) = (config.state_dir.clone(), path.to_owned());
    let _ = blocking(move || {
        place_records::note(state.as_deref(), |l| l.drop_path(&path));
        if let Err(e) = super::build_dirs::remove(&build) {
            tracing::warn!(error = %e, "cannot remove build folders");
        }
        Ok(())
    })
    .await;
}

async fn scan(config: &Config, holds: &Holds) -> Result<Vec<Found>, TaskError> {
    let (c, state) = (config.clone(), config.state_dir.clone());
    let (all, records) = blocking(move || {
        let records = match state.as_deref() {
            Some(dir) => place_records::read(dir).map_err(|e| e.to_string())?.places,
            None => Vec::new(),
        };
        Ok((c.load_workspaces()?, records))
    })
    .await
    .map_err(TaskError::Failed)?;
    let mut seen: HashSet<PathBuf> = HashSet::new();
    let mut out = Vec::new();
    let now = crate::records::now();
    for r in &records {
        if !r.path.exists() {
            continue;
        }
        seen.insert(r.path.clone());
        out.push(recorded(r, holds, now).await);
    }
    for (path, branch, main) in other_worktrees(&all).await {
        if seen.insert(path.clone()) {
            out.push(other(path, branch, &main, holds).await);
        }
    }
    let built: Vec<PathBuf> = records.iter().flat_map(|r| r.build.clone()).collect();
    let loose = blocking(move || Ok(loose_folders(&all, &seen, &built)))
        .await
        .unwrap_or_default();
    out.extend(loose.into_iter().map(|(kind, path, ws)| {
        let held = holds.of(&path);
        let why =
            (kind == DiskKind::TaskFolder).then(|| "a Task folder holds the user's files".into());
        found(kind, path, Some(ws), None, held, None, why, Vec::new())
    }));
    Ok(out)
}

async fn recorded(r: &PlaceRecord, holds: &Holds, now: u64) -> Found {
    let held = holds.of(&r.path);
    let young = now.saturating_sub(r.made) < GRACE;
    if r.kind != PlaceKind::Branch {
        let why = Some("a Task folder holds the user's files".into());
        return found(
            DiskKind::TaskFolder,
            r.path.clone(),
            Some(r.workspace.clone()),
            None,
            held,
            None,
            why,
            r.build.clone(),
        );
    }
    let (tree, state, mut why) = check(&r.path, r.branch.as_deref()).await;
    if young && why.is_none() {
        why = Some("the Task is starting".into());
    }
    let mut f = found(
        DiskKind::TaskWorktree,
        r.path.clone(),
        Some(r.workspace.clone()),
        r.branch.clone(),
        held,
        state,
        why,
        r.build.clone(),
    );
    f.tree = tree;
    f
}

async fn other(path: PathBuf, branch: Option<String>, main: &Path, holds: &Holds) -> Found {
    let kind = if path.starts_with(main.join(CLAUDE_WORKTREES)) {
        DiskKind::ClaudeWorktree
    } else {
        DiskKind::Worktree
    };
    let (tree, state, mut why) = check(&path, branch.as_deref()).await;
    if why.is_none() && !idle(&path).await {
        why = Some("changed in the last day".into());
    }
    let mut f = found(
        kind,
        path.clone(),
        None,
        branch,
        holds.of(&path),
        state,
        why,
        Vec::new(),
    );
    f.tree = tree;
    f
}

/// The worktree, whether it is clean and merged, and why it is not cleanable.
async fn check(
    path: &Path,
    branch: Option<&str>,
) -> (Option<Tree>, Option<close::WorkState>, Option<String>) {
    let Some(branch) = branch else {
        return (None, None, Some("not on a branch".into()));
    };
    let tree = match close::tree(path, branch).await {
        Ok(Some(t)) => t,
        Ok(None) => return (None, None, Some("no longer holds its branch".into())),
        Err(e) => return (None, None, Some(e.to_string())),
    };
    match close::work_state(&tree).await {
        Ok(state) => {
            let why = match (&state.clean, &state.unmerged) {
                (false, _) => Some("uncommitted changes".to_owned()),
                (true, Some(base)) => Some(format!("commits not in {base}")),
                (true, None) => None,
            };
            (Some(tree), Some(state), why)
        }
        Err(e) => (Some(tree), None, Some(e.to_string())),
    }
}

#[allow(clippy::too_many_arguments)]
fn found(
    kind: DiskKind,
    path: PathBuf,
    workspace: Option<String>,
    branch: Option<String>,
    held: Option<muxa_protocol::v2::Hold>,
    state: Option<close::WorkState>,
    why: Option<String>,
    build: Vec<PathBuf>,
) -> Found {
    Found {
        row: DiskRow {
            path: path.to_string_lossy().into_owned(),
            kind,
            workspace,
            branch,
            bytes: None,
            build_bytes: None,
            held,
            clean: state.as_ref().map(|s| s.clean),
            merged: state.as_ref().map(|s| s.unmerged.is_none()),
            cleanable: held.is_none() && why.is_none(),
        },
        tree: None,
        build,
        why,
    }
}

/// Worktrees of every Workspace repo, with the repo's main checkout.
async fn other_worktrees(all: &Workspaces) -> Vec<(PathBuf, Option<String>, PathBuf)> {
    let list: Vec<_> = all.all().to_vec();
    let mains = blocking(move || {
        let mut mains: Vec<PathBuf> = list.iter().flat_map(|w| w.repo_list()).collect();
        mains.sort();
        mains.dedup();
        Ok(mains)
    })
    .await
    .unwrap_or_default();
    let mut out = Vec::new();
    for main in mains {
        match super::git::worktrees(&main).await {
            Ok(trees) => out.extend(trees.into_iter().map(|(p, b)| (p, b, main.clone()))),
            Err(e) => tracing::warn!(repo = %main.display(), error = %e, "cannot list worktrees"),
        }
    }
    out
}

/// Task folders and build folders that no record names. Blocking.
fn loose_folders(
    all: &Workspaces,
    seen: &HashSet<PathBuf>,
    built: &[PathBuf],
) -> Vec<(DiskKind, PathBuf, String)> {
    let mut out = Vec::new();
    for ws in all.all() {
        for dir in subdirs(&ws.root.join(crate::records::workspaces::TASKS_DIR)) {
            if !seen.contains(&dir) {
                out.push((DiskKind::TaskFolder, dir, ws.name.clone()));
            }
        }
        let Some(disk) = all.build_disk(ws) else {
            continue;
        };
        for dir in subdirs(&disk.dir.join(&ws.name)) {
            if !built.iter().any(|b| b.starts_with(&dir)) {
                out.push((DiskKind::BuildFolder, dir, ws.name.clone()));
            }
        }
    }
    out
}

fn subdirs(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut out: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
        .map(|e| e.path())
        .collect();
    out.sort();
    out
}

/// The worktree's index has not changed for [`IDLE`].
async fn idle(path: &Path) -> bool {
    let out = super::git::run(
        super::git::git(path).args(["rev-parse", "--path-format=absolute", "--git-dir"]),
        "git rev-parse",
    )
    .await;
    let Some(dir) = out
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| super::git::last_line(&o.stdout))
    else {
        return false;
    };
    std::fs::metadata(Path::new(&dir).join("index"))
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| SystemTime::now().duration_since(t).ok())
        .is_some_and(|age| age >= IDLE)
}

/// `du -sk` of each row and of its build folders. Links are not followed.
async fn sizes(found: &mut [Found]) {
    let mut paths: Vec<PathBuf> = found.iter().map(|f| PathBuf::from(&f.row.path)).collect();
    paths.extend(found.iter().flat_map(|f| f.build.iter().cloned()));
    let measured = blocking(move || Ok(du(&paths))).await.unwrap_or_default();
    let size = |p: &Path| measured.iter().find(|(q, _)| q == p).map(|(_, b)| *b);
    for f in found {
        f.row.bytes = size(Path::new(&f.row.path));
        if !f.build.is_empty() {
            f.row.build_bytes = f.build.iter().filter_map(|b| size(b)).reduce(|a, b| a + b);
        }
    }
}

/// Blocking. Paths du cannot read are left out.
fn du(paths: &[PathBuf]) -> Vec<(PathBuf, u64)> {
    let existing: Vec<&PathBuf> = paths.iter().filter(|p| p.exists()).collect();
    if existing.is_empty() {
        return Vec::new();
    }
    let out = match std::process::Command::new("du")
        .arg("-sk")
        .args(&existing)
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .output()
    {
        Ok(o) => o,
        Err(e) => {
            tracing::warn!(error = %e, "cannot run du");
            return Vec::new();
        }
    };
    parse_du(&String::from_utf8_lossy(&out.stdout))
}

fn parse_du(text: &str) -> Vec<(PathBuf, u64)> {
    text.lines()
        .filter_map(|l| {
            let (kb, path) = l.split_once('\t')?;
            Some((PathBuf::from(path), kb.trim().parse::<u64>().ok()? * 1024))
        })
        .collect()
}

async fn blocking<T: Send + 'static>(
    f: impl FnOnce() -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    tokio::task::spawn_blocking(f)
        .await
        .map_err(|e| e.to_string())?
}

#[cfg(test)]
#[path = "disk_tests.rs"]
mod tests;
