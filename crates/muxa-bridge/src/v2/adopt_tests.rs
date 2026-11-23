use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use super::*;

pub(crate) fn temp(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("muxa-adopt-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir.canonicalize().unwrap()
}

pub(crate) fn git(dir: &Path, args: &[&str]) {
    let ok = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .status()
        .unwrap()
        .success();
    assert!(ok, "git {args:?}");
}

/// A repo with one commit on `main`.
pub(crate) fn repo(dir: &Path) {
    std::fs::create_dir_all(dir).unwrap();
    git(dir, &["init", "-q", "-b", "main"]);
    std::fs::write(dir.join("README.md"), "x\n").unwrap();
    git(dir, &["add", "."]);
    git(
        dir,
        &[
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@t",
            "commit",
            "-q",
            "-m",
            "init",
        ],
    );
}

fn workspace(root: &Path, repos: Vec<PathBuf>) -> Workspace {
    Workspace {
        name: "w".into(),
        root: root.to_owned(),
        repos,
        missing_repos: Vec::new(),
        hidden_repos: Vec::new(),
        agent: None,
        loop_checks: None,
        park_after: None,
        routines: None,
        connections: Vec::new(),
        mcp: Vec::new(),
        claude_connectors: false,
    }
}

fn path(p: &Path) -> String {
    p.to_string_lossy().into_owned()
}

#[tokio::test]
async fn a_folder_in_the_root_is_adopted_as_it_is() {
    let base = temp("folder");
    let dir = base.join("tasks/budget-t-000001");
    std::fs::create_dir_all(&dir).unwrap();
    let a = resolve(&workspace(&base, Vec::new()), &path(&dir))
        .await
        .unwrap();
    assert_eq!(
        a,
        Adopted {
            path: dir.clone(),
            main: None,
            branch: None,
            in_repo: false,
        }
    );
    assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 0, "nothing made");
    std::fs::remove_dir_all(base).unwrap();
}

#[tokio::test]
async fn a_worktree_of_a_listed_repo_keeps_its_branch_outside_the_root() {
    let base = temp("worktree");
    let (root, src) = (base.join("root"), base.join("src"));
    std::fs::create_dir_all(&root).unwrap();
    repo(&src.join("app"));
    let tree = src.join(".worktrees/app-fix");
    git(
        &src.join("app"),
        &["worktree", "add", "-q", "-b", "fix", &path(&tree)],
    );
    let ws = workspace(&root, vec![src.join("app")]);
    let a = resolve(&ws, &path(&tree)).await.unwrap();
    assert_eq!(a.branch.as_deref(), Some("fix"));
    assert_eq!(a.main.as_deref(), Some(src.join("app").as_path()));
    assert!(a.in_repo);

    // The main checkout of a repo outside the root is not a worktree to adopt.
    let refused = resolve(&ws, &path(&src.join("app"))).await;
    assert!(
        matches!(&refused, Err(TaskError::Invalid(m)) if m.contains("outside Workspace w")),
        "{refused:?}"
    );
    std::fs::remove_dir_all(base).unwrap();
}

#[tokio::test]
async fn a_path_outside_through_a_symlink_or_relative_is_refused() {
    let base = temp("outside");
    let (root, elsewhere) = (base.join("root"), base.join("elsewhere"));
    std::fs::create_dir_all(&root).unwrap();
    std::fs::create_dir_all(&elsewhere).unwrap();
    std::os::unix::fs::symlink(&elsewhere, root.join("link")).unwrap();
    let ws = workspace(&root, Vec::new());
    for raw in [
        path(&elsewhere),
        path(&root.join("link")),
        path(&root.join("../elsewhere")),
    ] {
        let refused = resolve(&ws, &raw).await;
        assert!(
            matches!(&refused, Err(TaskError::Invalid(m)) if m.contains("outside")),
            "{raw}: {refused:?}"
        );
    }
    for raw in ["tasks/x".to_owned(), path(&root.join("gone"))] {
        assert!(
            matches!(resolve(&ws, &raw).await, Err(TaskError::Invalid(_))),
            "{raw}"
        );
    }
    std::fs::remove_dir_all(base).unwrap();
}

#[test]
fn an_agent_is_found_by_its_program_and_working_directory() {
    let base = temp("procs");
    let procs = Processes::parse(
        " 10 1 01:00 /usr/bin/claude\n 11 1 01:00 /opt/bin/codex\n 12 1 01:00 /bin/zsh\n",
    );
    let (work, other) = (base.join("work"), base.join("other"));
    std::fs::create_dir_all(&work).unwrap();
    let cwds = |cwd_of_10: PathBuf, cwd_of_11: PathBuf| {
        move |pids: &[u32]| -> Result<HashMap<u32, PathBuf>, agents::Error> {
            assert!(!pids.contains(&12), "only agent programs are asked");
            Ok(HashMap::from([
                (10, cwd_of_10.clone()),
                (11, cwd_of_11.clone()),
            ]))
        }
    };
    let found = agents::agent_in(&work, &procs, &cwds(other.clone(), work.clone())).unwrap();
    assert_eq!(found, Some(("codex".to_owned(), 11)));
    let none = agents::agent_in(&work, &procs, &cwds(other.clone(), other.clone())).unwrap();
    assert_eq!(none, None);
    std::fs::remove_dir_all(base).unwrap();
}
