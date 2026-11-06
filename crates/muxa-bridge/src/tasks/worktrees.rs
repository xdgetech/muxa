//! muxa's list of the worktrees it created, and their removal (ADR 0043).
//! muxa for tmux's `lib.sh` and `close.sh` keep the same list and rules; the
//! contract test checks that both agree.

use std::path::{Path, PathBuf};
use std::time::Duration;

use super::{TaskError, git, last_line, run};

const GIT_TIMEOUT: Duration = Duration::from_secs(20);

/// `$XDG_STATE_HOME/muxa/worktrees`, else `~/.local/state/muxa/worktrees`.
/// A relative `XDG_STATE_HOME` is ignored, as the XDG spec says.
pub fn dir() -> Option<PathBuf> {
    // Unit tests create worktrees; their list must not reach the developer's.
    if cfg!(test) {
        return Some(
            std::env::temp_dir().join(format!("muxa-unit-worktrees-{}", std::process::id())),
        );
    }
    let state = std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| {
            std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local").join("state"))
        })?;
    Some(state.join("muxa").join("worktrees"))
}

/// One file per worktree, named after its resolved path with '/' as '%'.
fn entry(dir: &Path, real: &Path) -> PathBuf {
    dir.join(real.to_string_lossy().replace('/', "%"))
}

/// Lists `worktree`, which muxa just created.
pub fn register(worktree: &Path) {
    let Some(dir) = dir() else { return };
    let result = worktree.canonicalize().and_then(|real| {
        std::fs::create_dir_all(&dir)?;
        std::fs::write(entry(&dir, &real), format!("{}\n", real.display()))
    });
    if let Err(e) = result {
        tracing::warn!(worktree = %worktree.display(), error = %e, "cannot list the new worktree");
    }
}

/// The listed worktrees, as their files hold them. Blocking.
pub fn listed() -> Vec<PathBuf> {
    let Some(dir) = dir() else { return Vec::new() };
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return Vec::new();
    };
    entries
        .filter_map(Result::ok)
        .filter(|e| !e.file_name().to_string_lossy().starts_with('.'))
        .filter_map(|e| std::fs::read_to_string(e.path()).ok())
        .map(|text| PathBuf::from(text.trim_end()))
        .filter(|p| p.is_absolute())
        .collect()
}

/// `real` is resolved before the worktree goes: afterwards it cannot be.
pub fn unregister(real: &Path) {
    let Some(dir) = dir() else { return };
    match std::fs::remove_file(entry(&dir, real)) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => {
            tracing::warn!(worktree = %real.display(), error = %e, "cannot unlist the worktree")
        }
    }
}

/// Why removing `worktree` could lose work, or None.
pub async fn risk(worktree: &Path, main: &Path) -> Result<Option<&'static str>, TaskError> {
    let out = run(
        git(worktree).args(["status", "--porcelain"]),
        GIT_TIMEOUT,
        "git status",
    )
    .await?;
    if !out.status.success() || !out.stdout.is_empty() {
        return Ok(Some("it has uncommitted changes"));
    }
    if !merged(worktree, main).await {
        return Ok(Some("its branch is not merged"));
    }
    Ok(None)
}

/// The branch adds nothing to the repo's default branch.
async fn merged(worktree: &Path, main: &Path) -> bool {
    let Ok(base) = default_base(main).await else {
        return false;
    };
    run(
        git(worktree).args(["merge-base", "--is-ancestor", "HEAD", &base]),
        GIT_TIMEOUT,
        "git merge-base",
    )
    .await
    .is_ok_and(|o| o.status.success())
}

/// origin's default branch, else the main checkout's current one.
pub(crate) async fn default_base(main: &Path) -> Result<String, TaskError> {
    for args in [
        &["symbolic-ref", "-q", "--short", "refs/remotes/origin/HEAD"][..],
        &["rev-parse", "--abbrev-ref", "HEAD"][..],
    ] {
        let out = run(git(main).args(args), GIT_TIMEOUT, "git").await?;
        if let Some(base) = last_line(&out.stdout).filter(|_| out.status.success()) {
            return Ok(base);
        }
    }
    Err(TaskError::Failed(
        "the repo has no default branch to start from; give a base".into(),
    ))
}

/// What a removal needs, read while the worktree still exists.
pub struct Removal {
    main: PathBuf,
    worktree: PathBuf,
    real: Option<PathBuf>,
    branch: Option<String>,
    base: Option<String>,
    force: bool,
}

impl Removal {
    pub async fn read(main: &Path, worktree: &Path, force: bool) -> Self {
        let branch = run(
            git(worktree).args(["branch", "--show-current"]),
            GIT_TIMEOUT,
            "git branch",
        )
        .await
        .ok()
        .and_then(|o| last_line(&o.stdout));
        Self {
            main: main.to_path_buf(),
            worktree: worktree.to_path_buf(),
            real: worktree.canonicalize().ok(),
            branch,
            base: default_base(main).await.ok(),
            force,
        }
    }

    /// Removes the worktree, unlists it, then deletes its branch. Without
    /// `force`, git refuses a worktree with changes, and the branch goes only
    /// when each of its commits is in the base branch or on a remote.
    pub async fn run(self, session: &str) -> Result<(), TaskError> {
        let mut cmd = git(&self.main);
        cmd.args(["worktree", "remove"]);
        if self.force {
            cmd.arg("--force");
        }
        let out = run(cmd.arg(&self.worktree), GIT_TIMEOUT, "git worktree remove").await?;
        if !out.status.success() {
            let why = last_line(&out.stderr).unwrap_or_default();
            return Err(TaskError::Failed(format!(
                "closed {session}, but could not remove {}: {}",
                self.worktree.display(),
                why.trim_start_matches("fatal: ")
            )));
        }
        if let Some(real) = &self.real {
            unregister(real);
        }
        self.delete_branch().await;
        Ok(())
    }

    /// A branch that stays is logged, not an error: the worktree is gone.
    async fn delete_branch(&self) {
        let Some(branch) = &self.branch else { return };
        if !self.force && self.has_unshared_commits(branch).await {
            tracing::info!(branch, "kept the branch: it has commits on no remote");
            return;
        }
        match run(
            git(&self.main).args(["branch", "-D", branch]),
            GIT_TIMEOUT,
            "git branch -D",
        )
        .await
        {
            Ok(o) if o.status.success() => {}
            Ok(o) => tracing::warn!(branch, stderr = o.stderr.len(), "cannot delete the branch"),
            Err(e) => tracing::warn!(branch, error = %e, "cannot delete the branch"),
        }
    }

    async fn has_unshared_commits(&self, branch: &str) -> bool {
        let mut cmd = git(&self.main);
        cmd.args([
            "rev-list",
            "-n",
            "1",
            &format!("refs/heads/{branch}"),
            "--not",
        ]);
        if let Some(base) = &self.base {
            cmd.arg(base);
        }
        cmd.arg("--remotes");
        match run(&mut cmd, GIT_TIMEOUT, "git rev-list").await {
            Ok(o) if o.status.success() => !o.stdout.is_empty(),
            // Unknown: keep the branch.
            _ => true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_entry_is_named_after_the_path() {
        let e = entry(Path::new("/s"), Path::new("/a/.worktrees/app-x"));
        assert_eq!(e, Path::new("/s/%a%.worktrees%app-x"));
    }
}
