//! git for a Task's places (ADR 0068): checking a branch request, finding or
//! making the branch's worktree, and writing `.muxa/` files that git ignores.
//! git never prompts: the daemon has no terminal to answer on.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Output;
use std::time::Duration;

use tokio::process::Command;

const GIT_TIMEOUT: Duration = Duration::from_secs(20);
const MAX_REF: usize = 200;
const MAX_MESSAGE: usize = 300;
/// The folder of muxa's files in a working place.
pub const DIR: &str = ".muxa";
const EXCLUDE: &str = ".muxa/";

#[derive(Debug, thiserror::Error)]
pub enum GitError {
    /// The request is wrong; the message says what to change.
    #[error("{0}")]
    Invalid(String),
    #[error("{0}")]
    Failed(String),
}

/// A checked branch place.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BranchRequest {
    /// Absolute; inside a git repo.
    pub repo: PathBuf,
    pub branch: String,
    pub base: Option<String>,
}

/// A branch's worktree and the main checkout it belongs to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Worktree {
    pub main: PathBuf,
    pub path: PathBuf,
}

/// Checks a branch place at the boundary.
pub async fn validate(
    repo: &str,
    branch: &str,
    base: Option<&str>,
) -> Result<BranchRequest, GitError> {
    let repo = PathBuf::from(repo);
    if !repo.is_absolute() || !repo.is_dir() {
        return Err(GitError::Invalid(format!(
            "{} is not a directory; give the repo's absolute path",
            repo.display()
        )));
    }
    check_ref(&repo, branch, "branch").await?;
    if let Some(base) = base {
        check_ref(&repo, base, "base").await?;
    }
    let inside = git(&repo)
        .args(["rev-parse", "--git-common-dir"])
        .output()
        .await
        .is_ok_and(|o| o.status.success());
    if !inside {
        return Err(GitError::Invalid(format!(
            "{} is not in a git repository",
            repo.display()
        )));
    }
    Ok(BranchRequest {
        repo,
        branch: branch.to_owned(),
        base: base.map(str::to_owned),
    })
}

/// A name git accepts for a branch, and no option in disguise.
async fn check_ref(repo: &Path, name: &str, what: &str) -> Result<(), GitError> {
    let invalid = || GitError::Invalid(format!("{name:?} is not a valid {what} name"));
    if name.is_empty() || name.len() > MAX_REF || name.starts_with('-') {
        return Err(invalid());
    }
    let ok = git(repo)
        .args(["check-ref-format", "--branch", name])
        .output()
        .await
        .is_ok_and(|o| o.status.success());
    if ok { Ok(()) } else { Err(invalid()) }
}

/// The branch's worktree: an existing one, or a new one.
pub async fn worktree(req: &BranchRequest) -> Result<Worktree, GitError> {
    let main = main_checkout(&req.repo).await?;
    if let Some(path) = existing_worktree(&main, &req.branch).await? {
        return Ok(Worktree { main, path });
    }
    if current_branch(&main).await.as_deref() == Some(req.branch.as_str()) {
        return Err(GitError::Invalid(format!(
            "{} is checked out in the main checkout {}. Switch it to another branch there, then start the task again.",
            req.branch,
            main.display()
        )));
    }
    let path = worktree_path(&main, &req.branch);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| GitError::Failed(format!("cannot create {}: {e}", parent.display())))?;
    }
    // A failed fetch leaves only local branches to choose from.
    let _ = run(git(&main).args(["fetch", "-q", "origin"]), "git fetch").await;
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
    let out = run(&mut add, "git worktree add").await?;
    if !out.status.success() {
        let why = last_line(&out.stderr).unwrap_or_default();
        let why = why.trim_start_matches("fatal: ");
        return Err(GitError::Failed(format!(
            "could not create a worktree for {}: {why}",
            req.branch
        )));
    }
    Ok(Worktree { main, path })
}

/// `<repos dir>/.worktrees/<repo>-<branch, '/' as '-'>`.
pub fn worktree_path(main: &Path, branch: &str) -> PathBuf {
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
pub async fn main_checkout(dir: &Path) -> Result<PathBuf, GitError> {
    let out = run(
        git(dir).args(["rev-parse", "--path-format=absolute", "--git-common-dir"]),
        "git rev-parse",
    )
    .await?;
    let common = last_line(&out.stdout)
        .filter(|_| out.status.success())
        .ok_or_else(|| {
            GitError::Invalid(format!("{} is not in a git repository", dir.display()))
        })?;
    Path::new(&common)
        .parent()
        .map(Path::to_path_buf)
        .ok_or_else(|| GitError::Failed(format!("unexpected git directory {common}")))
}

pub(super) async fn existing_worktree(
    main: &Path,
    branch: &str,
) -> Result<Option<PathBuf>, GitError> {
    let out = run(
        git(main).args(["worktree", "list", "--porcelain"]),
        "git worktree list",
    )
    .await?;
    Ok(find_worktree(
        &String::from_utf8_lossy(&out.stdout),
        main,
        branch,
    ))
}

/// Every worktree of `main` but the main checkout, with its branch (None
/// when its HEAD is detached).
pub(super) async fn worktrees(main: &Path) -> Result<Vec<(PathBuf, Option<String>)>, GitError> {
    let out = run(
        git(main).args(["worktree", "list", "--porcelain"]),
        "git worktree list",
    )
    .await?;
    Ok(parse_worktrees(&String::from_utf8_lossy(&out.stdout), main))
}

fn parse_worktrees(porcelain: &str, main: &Path) -> Vec<(PathBuf, Option<String>)> {
    let mut out: Vec<(PathBuf, Option<String>)> = Vec::new();
    for line in porcelain.lines() {
        if let Some(p) = line.strip_prefix("worktree ") {
            out.push((PathBuf::from(p), None));
        } else if let Some(b) = line.strip_prefix("branch refs/heads/")
            && let Some(last) = out.last_mut()
        {
            last.1 = Some(b.to_owned());
        }
    }
    out.retain(|(p, _)| p != main);
    out
}

/// A worktree other than the main checkout that has `branch` checked out.
fn find_worktree(porcelain: &str, main: &Path, branch: &str) -> Option<PathBuf> {
    let want = format!("refs/heads/{branch}");
    let mut path: Option<&str> = None;
    for line in porcelain.lines() {
        if let Some(p) = line.strip_prefix("worktree ") {
            path = Some(p);
        } else if line.strip_prefix("branch ") == Some(want.as_str())
            && let Some(p) = path.filter(|p| Path::new(p) != main)
        {
            return Some(PathBuf::from(p));
        }
    }
    None
}

/// The top folder of the worktree that holds `dir`; None outside git.
pub(super) async fn toplevel(dir: &Path) -> Result<Option<PathBuf>, GitError> {
    let out = run(
        git(dir).args(["rev-parse", "--show-toplevel"]),
        "git rev-parse",
    )
    .await?;
    Ok(last_line(&out.stdout)
        .filter(|_| out.status.success())
        .map(PathBuf::from))
}

pub(super) async fn current_branch(dir: &Path) -> Option<String> {
    let out = run(git(dir).args(["branch", "--show-current"]), "git branch")
        .await
        .ok()?;
    last_line(&out.stdout)
}

async fn has_ref(dir: &Path, name: &str) -> bool {
    run(
        git(dir).args(["show-ref", "--verify", "-q", name]),
        "git show-ref",
    )
    .await
    .is_ok_and(|o| o.status.success())
}

/// `origin/HEAD`, else the main checkout's branch.
pub(super) async fn default_base(main: &Path) -> Result<String, GitError> {
    for args in [
        &["symbolic-ref", "-q", "--short", "refs/remotes/origin/HEAD"][..],
        &["rev-parse", "--abbrev-ref", "HEAD"][..],
    ] {
        let out = run(git(main).args(args), "git").await?;
        if let Some(base) = last_line(&out.stdout).filter(|_| out.status.success()) {
            return Ok(base);
        }
    }
    Err(GitError::Failed(
        "the repo has no default branch to start from; give a base".into(),
    ))
}

pub(super) fn git(dir: &Path) -> Command {
    let mut cmd = Command::new("git");
    cmd.arg("-C")
        .arg(dir)
        .env("GIT_TERMINAL_PROMPT", "0")
        .stdin(std::process::Stdio::null())
        .kill_on_drop(true);
    if std::env::var_os("GIT_SSH_COMMAND").is_none() {
        cmd.env("GIT_SSH_COMMAND", "ssh -o BatchMode=yes");
    }
    cmd
}

/// Runs `cmd` to the end, or kills it after [`GIT_TIMEOUT`].
pub(super) async fn run(cmd: &mut Command, what: &str) -> Result<Output, GitError> {
    match tokio::time::timeout(GIT_TIMEOUT, cmd.output()).await {
        Ok(Ok(out)) => Ok(out),
        Ok(Err(e)) => Err(GitError::Failed(format!("cannot run {what}: {e}"))),
        Err(_) => Err(GitError::Failed(format!(
            "{what} did not finish in {} seconds",
            GIT_TIMEOUT.as_secs()
        ))),
    }
}

/// The last non-empty line of a command's output, shortened for a message.
pub(super) fn last_line(bytes: &[u8]) -> Option<String> {
    let text = String::from_utf8_lossy(bytes);
    let line = text.lines().map(str::trim).rfind(|l| !l.is_empty())?;
    Some(line.chars().take(MAX_MESSAGE).collect())
}

/// Writes `<dir>/.muxa/<name>` through a temp file and a rename, so the
/// agent never reads half of it. Blocking.
pub fn write_file(dir: &Path, name: &str, text: &str) -> Result<PathBuf, String> {
    let folder = dir.join(DIR);
    std::fs::create_dir_all(&folder)
        .map_err(|e| format!("cannot create {}: {e}", folder.display()))?;
    let path = folder.join(name);
    let tmp = folder.join(format!(".{name}.tmp"));
    std::fs::write(&tmp, text).map_err(|e| format!("cannot write {}: {e}", tmp.display()))?;
    std::fs::rename(&tmp, &path).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        format!("cannot write {}: {e}", path.display())
    })?;
    Ok(path)
}

/// Adds `text` to the end of `.muxa/<name>` in `dir`. Blocking.
pub fn append_file(dir: &Path, name: &str, text: &str) -> Result<PathBuf, String> {
    let path = dir.join(DIR).join(name);
    let before = std::fs::read_to_string(&path).unwrap_or_default();
    write_file(dir, name, &format!("{before}{text}"))
}

/// Adds `.muxa/` to the repo's `info/exclude`, shared by all its worktrees.
/// Blocking.
pub fn exclude(dir: &Path) -> Result<(), String> {
    exclude_line(dir, EXCLUDE)
}

/// Adds `line` to the repo's `info/exclude` unless it is there. Blocking.
pub fn exclude_line(dir: &Path, line: &str) -> Result<(), String> {
    let out = std::process::Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(["rev-parse", "--path-format=absolute", "--git-common-dir"])
        .env("GIT_OPTIONAL_LOCKS", "0")
        .stdin(std::process::Stdio::null())
        .output()
        .map_err(|e| format!("cannot run git: {e}"))?;
    if !out.status.success() {
        return Err(format!("{} is not in a git repository", dir.display()));
    }
    let common = String::from_utf8_lossy(&out.stdout);
    let info = Path::new(common.trim_end_matches('\n')).join("info");
    let path = info.join("exclude");
    let existing = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(format!("cannot read {}: {e}", path.display())),
    };
    let Some(addition) = exclude_addition(&existing, line) else {
        return Ok(());
    };
    std::fs::create_dir_all(&info).map_err(|e| format!("cannot create {}: {e}", info.display()))?;
    std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .and_then(|mut f| f.write_all(addition.as_bytes()))
        .map_err(|e| format!("cannot write {}: {e}", path.display()))
}

/// What to append to an exclude file, or None when it already has the line.
fn exclude_addition(existing: &str, line: &str) -> Option<String> {
    let present = existing
        .lines()
        .map(str::trim)
        .any(|l| l == line || (line == EXCLUDE && (l == "/.muxa/" || l == ".muxa")));
    if present {
        return None;
    }
    let lead = if existing.is_empty() || existing.ends_with('\n') {
        ""
    } else {
        "\n"
    };
    Some(format!("{lead}{line}\n"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_worktree_is_found_by_its_branch_but_never_the_main_checkout() {
        let porcelain = "worktree /r\nHEAD 1\nbranch refs/heads/main\n\n\
                         worktree /w/r-fix\nHEAD 2\nbranch refs/heads/fix\n";
        assert_eq!(
            find_worktree(porcelain, Path::new("/r"), "fix"),
            Some("/w/r-fix".into())
        );
        assert_eq!(find_worktree(porcelain, Path::new("/r"), "main"), None);
        let detached = format!("{porcelain}\nworktree /w/r-x\nHEAD 3\ndetached\n");
        assert_eq!(
            parse_worktrees(&detached, Path::new("/r")),
            vec![
                (PathBuf::from("/w/r-fix"), Some("fix".into())),
                (PathBuf::from("/w/r-x"), None)
            ]
        );
        assert_eq!(
            worktree_path(Path::new("/src/app"), "fix/login"),
            PathBuf::from("/src/.worktrees/app-fix-login")
        );
    }

    #[test]
    fn the_exclude_line_is_added_once() {
        assert_eq!(exclude_addition("", EXCLUDE), Some(".muxa/\n".into()));
        assert_eq!(
            exclude_addition("target", EXCLUDE),
            Some("\n.muxa/\n".into())
        );
        assert_eq!(exclude_addition("/.muxa/\n", EXCLUDE), None);
    }
}
