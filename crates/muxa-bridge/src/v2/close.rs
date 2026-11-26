//! Closing a Task's places (ADR 0068 §7, ADR 0043): a branch worktree goes
//! with its branch when it is clean and merged, or on `force`. Only the
//! worktrees on the agent's `muxa.places` label are touched, because muxa
//! made them. Read-only places and the folder always stay.

use std::path::{Path, PathBuf};

use muxa_protocol::v2::{Place, PlaceKind, WorktreeMode};
use muxa_server::TaskError;

use super::git::{self, GitError};

/// A Task's branch worktree that is still there.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Tree {
    pub main: PathBuf,
    pub path: PathBuf,
    pub branch: String,
}

/// With `remove`, refuses when a worktree would lose work and says which.
pub async fn check(places: &[Place], mode: WorktreeMode) -> Result<(), TaskError> {
    if mode == WorktreeMode::Force {
        return Ok(());
    }
    let mut losses = Vec::new();
    for t in trees(places).await? {
        losses.extend(losses_of(&t).await?);
    }
    if losses.is_empty() {
        return Ok(());
    }
    Err(TaskError::WouldLoseWork(format!(
        "{}. Approve with force to remove them anyway",
        losses.join("; ")
    )))
}

/// Removes each branch worktree and its branch. With `remove` the caller
/// checked first, so each branch is merged by muxa's rule.
pub async fn remove(places: &[Place], mode: WorktreeMode) -> Result<(), TaskError> {
    let mut problems = Vec::new();
    for t in trees(places).await? {
        if let Err(e) = remove_tree(&t, mode).await {
            problems.push(e);
        }
    }
    if problems.is_empty() {
        Ok(())
    } else {
        Err(TaskError::Failed(problems.join("; ")))
    }
}

/// The branch places whose worktree still holds their branch. A place that
/// is gone, or a main checkout, is skipped.
async fn trees(places: &[Place]) -> Result<Vec<Tree>, TaskError> {
    let mut out = Vec::new();
    for p in places.iter().filter(|p| p.kind == PlaceKind::Branch) {
        let Some(branch) = &p.branch else { continue };
        if let Some(t) = tree(Path::new(&p.path), branch).await? {
            out.push(t);
        }
    }
    Ok(out)
}

/// The worktree at `path` if it still holds `branch` and is not a main
/// checkout.
pub(crate) async fn tree(path: &Path, branch: &str) -> Result<Option<Tree>, TaskError> {
    if !path.is_dir() {
        return Ok(None);
    }
    let main = git::main_checkout(path).await.map_err(failed)?;
    let found = git::existing_worktree(&main, branch)
        .await
        .map_err(failed)?;
    if !same(found.as_deref(), path) || same(Some(&main), path) {
        tracing::warn!(path = %path.display(), "a Task place no longer holds its branch; kept");
        return Ok(None);
    }
    Ok(Some(Tree {
        main,
        path: path.to_owned(),
        branch: branch.to_owned(),
    }))
}

fn same(a: Option<&Path>, b: &Path) -> bool {
    let canon = |p: &Path| p.canonicalize().unwrap_or_else(|_| p.to_owned());
    a.is_some_and(|a| canon(a) == canon(b))
}

/// Whether a worktree holds work that removing it would lose.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct WorkState {
    pub clean: bool,
    /// The base the branch is not in; None when merged.
    pub unmerged: Option<String>,
}

/// What closing `t` would lose: uncommitted changes, commits not merged.
pub(crate) async fn losses_of(t: &Tree) -> Result<Vec<String>, TaskError> {
    let state = work_state(t).await?;
    let mut out = Vec::new();
    if !state.clean {
        out.push(format!(
            "{} has uncommitted changes in {}",
            t.branch,
            t.path.display()
        ));
    }
    if let Some(base) = state.unmerged {
        out.push(format!("{} has commits that are not in {base}", t.branch));
    }
    Ok(out)
}

pub(crate) async fn work_state(t: &Tree) -> Result<WorkState, TaskError> {
    let status = git::run(
        git::git(&t.path).args(["status", "--porcelain"]),
        "git status",
    )
    .await
    .map_err(failed)?;
    if !status.status.success() {
        return Err(TaskError::Failed(format!(
            "cannot read the status of {}",
            t.path.display()
        )));
    }
    Ok(WorkState {
        clean: status.stdout.is_empty(),
        unmerged: unmerged(t).await?,
    })
}

/// None when the branch is in the default base or the main checkout's
/// branch (ADR 0043 §2); else the base it was compared with.
async fn unmerged(t: &Tree) -> Result<Option<String>, TaskError> {
    let mut bases = vec![git::default_base(&t.main).await.map_err(failed)?];
    if let Some(current) = git::current_branch(&t.main).await {
        bases.push(current);
    }
    bases.dedup();
    let tip = format!("refs/heads/{}", t.branch);
    for base in &bases {
        let out = git::run(
            git::git(&t.main).args(["merge-base", "--is-ancestor", &tip, base]),
            "git merge-base",
        )
        .await
        .map_err(failed)?;
        if out.status.success() {
            return Ok(None);
        }
    }
    Ok(bases.into_iter().next())
}

pub(crate) async fn remove_tree(t: &Tree, mode: WorktreeMode) -> Result<(), String> {
    let mut rm = git::git(&t.main);
    rm.args(["worktree", "remove"]);
    if mode == WorktreeMode::Force {
        rm.arg("--force");
    }
    rm.arg(&t.path);
    expect_ok(
        &mut rm,
        &format!("remove the worktree {}", t.path.display()),
    )
    .await?;
    let flag = match mode {
        WorktreeMode::Remove => "-d",
        WorktreeMode::Force => "-D",
    };
    let what = format!("delete the branch {}", t.branch);
    let deleted = expect_ok(git::git(&t.main).args(["branch", flag, &t.branch]), &what).await;
    match deleted {
        // Merged into origin's base but not the local one, which `-d` checks.
        Err(_) if mode == WorktreeMode::Remove => {
            expect_ok(git::git(&t.main).args(["branch", "-D", &t.branch]), &what).await
        }
        other => other,
    }
}

async fn expect_ok(cmd: &mut tokio::process::Command, what: &str) -> Result<(), String> {
    let out = git::run(cmd, what).await.map_err(|e| e.to_string())?;
    if out.status.success() {
        return Ok(());
    }
    let why = git::last_line(&out.stderr).unwrap_or_default();
    Err(format!(
        "could not {what}: {}",
        why.trim_start_matches("fatal: ")
    ))
}

fn failed(e: GitError) -> TaskError {
    TaskError::Failed(e.to_string())
}

#[cfg(test)]
#[path = "close_tests.rs"]
mod tests;
