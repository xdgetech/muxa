//! Starts tasks without tmux-agents: a worktree where tmux-agents would put it,
//! and a detached session running the agent in one pane, tagged for muxa.
//! No layout and no sidebar; those belong to tmux-agents.

use std::path::{Path, PathBuf};
use std::time::Duration;

use muxa_tmux::Client;

use super::{
    LAUNCH_TIMEOUT, Launch, TaskError, TaskLauncher, TaskRequest, git, last_line, run, session_name,
};

const GIT_TIMEOUT: Duration = Duration::from_secs(20);

pub struct Builtin {
    command: String,
}

impl Builtin {
    pub fn new(command: String) -> Self {
        Self { command }
    }
}

impl TaskLauncher for Builtin {
    fn id(&self) -> &'static str {
        "builtin"
    }

    fn launch<'a>(&'a self, client: &'a Client, req: &'a TaskRequest) -> Launch<'a> {
        Box::pin(async move {
            let work = tokio::time::timeout(LAUNCH_TIMEOUT, worktree(req))
                .await
                .map_err(|_| TaskError::Failed("creating the worktree took too long".into()))??;
            let name = session_name(&req.branch);
            if muxa_tmux::session_names(client).await?.contains(&name) {
                return Ok(name);
            }
            let dir = work.to_string_lossy();
            let pane = muxa_tmux::new_session(client, &name, &dir, &self.command).await?;
            muxa_tmux::set_pane_option(client, pane, "@muxa_role", "agent").await?;
            Ok(name)
        })
    }
}

/// The branch's worktree: an existing one, or a new one.
pub(crate) async fn worktree(req: &TaskRequest) -> Result<PathBuf, TaskError> {
    let main = main_checkout(&req.repo).await?;
    if let Some(found) = existing_worktree(&main, &req.branch).await? {
        return Ok(found);
    }
    if current_branch(&main).await.as_deref() == Some(req.branch.as_str()) {
        return Err(TaskError::Invalid(format!(
            "{} is checked out in the main checkout {}. Switch it to another branch there, then start the task again.",
            req.branch,
            main.display()
        )));
    }
    let path = worktree_path(&main, &req.branch);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| TaskError::Failed(format!("cannot create {}: {e}", parent.display())))?;
    }
    // A failed fetch leaves only local branches to choose from.
    let _ = run(
        git(&main).args(["fetch", "-q", "origin"]),
        GIT_TIMEOUT,
        "git fetch",
    )
    .await;

    let mut add = git(&main);
    add.args(["worktree", "add", "-q"]);
    if has_ref(&main, &format!("refs/heads/{}", req.branch)).await {
        add.arg(&path).arg(&req.branch);
    } else if has_ref(&main, &format!("refs/remotes/origin/{}", req.branch)).await {
        add.args(["--track", "-b", &req.branch])
            .arg(&path)
            .arg(format!("origin/{}", req.branch));
    } else {
        let base = match &req.base {
            Some(b) => b.clone(),
            None => default_base(&main).await?,
        };
        add.args(["-b", &req.branch]).arg(&path).arg(base);
    }
    let out = run(&mut add, GIT_TIMEOUT, "git worktree add").await?;
    if !out.status.success() {
        let why = last_line(&out.stderr).unwrap_or_default();
        let why = why.trim_start_matches("fatal: ");
        return Err(TaskError::Failed(format!(
            "could not create a worktree for {}: {why}",
            req.branch
        )));
    }
    Ok(path)
}

/// `<repos dir>/.worktrees/<repo>-<branch, '/' as '-'>`, as tmux-agents names it.
pub(crate) fn worktree_path(main: &Path, branch: &str) -> PathBuf {
    let repo = main
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let parent = main.parent().unwrap_or(main);
    parent
        .join(".worktrees")
        .join(format!("{repo}-{}", branch.replace('/', "-")))
}

/// The main checkout of the repo that holds `dir`, also from inside a worktree.
pub(crate) async fn main_checkout(dir: &Path) -> Result<PathBuf, TaskError> {
    let out = run(
        git(dir).args(["rev-parse", "--path-format=absolute", "--git-common-dir"]),
        GIT_TIMEOUT,
        "git rev-parse",
    )
    .await?;
    let common = last_line(&out.stdout)
        .filter(|_| out.status.success())
        .ok_or_else(|| {
            TaskError::Invalid(format!("{} is not in a git repository", dir.display()))
        })?;
    Path::new(&common)
        .parent()
        .map(Path::to_path_buf)
        .ok_or_else(|| TaskError::Failed(format!("unexpected git directory {common}")))
}

/// A worktree other than the main checkout that has `branch` checked out.
async fn existing_worktree(main: &Path, branch: &str) -> Result<Option<PathBuf>, TaskError> {
    let out = run(
        git(main).args(["worktree", "list", "--porcelain"]),
        GIT_TIMEOUT,
        "git worktree list",
    )
    .await?;
    Ok(find_worktree(
        &String::from_utf8_lossy(&out.stdout),
        main,
        branch,
    ))
}

pub(crate) fn find_worktree(porcelain: &str, main: &Path, branch: &str) -> Option<PathBuf> {
    let want = format!("refs/heads/{branch}");
    let mut path: Option<&str> = None;
    for line in porcelain.lines() {
        if let Some(p) = line.strip_prefix("worktree ") {
            path = Some(p);
        } else if line.strip_prefix("branch ") == Some(want.as_str()) {
            if let Some(p) = path.filter(|p| Path::new(p) != main) {
                return Some(PathBuf::from(p));
            }
        }
    }
    None
}

async fn current_branch(dir: &Path) -> Option<String> {
    let out = run(
        git(dir).args(["branch", "--show-current"]),
        GIT_TIMEOUT,
        "git branch",
    )
    .await
    .ok()?;
    last_line(&out.stdout)
}

async fn has_ref(dir: &Path, name: &str) -> bool {
    run(
        git(dir).args(["show-ref", "--verify", "-q", name]),
        GIT_TIMEOUT,
        "git show-ref",
    )
    .await
    .is_ok_and(|o| o.status.success())
}

/// origin's default branch, else the main checkout's current one.
async fn default_base(main: &Path) -> Result<String, TaskError> {
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
