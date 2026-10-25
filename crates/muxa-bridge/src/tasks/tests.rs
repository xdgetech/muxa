use std::path::{Path, PathBuf};
use std::process::Command;

use super::builtin::{find_worktree, main_checkout, worktree_path};
use super::*;

/// A repo at `<tmp>/repos/app` with one commit on `main`.
fn repo(name: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("muxa-tasks-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let app = root.join("repos/app");
    std::fs::create_dir_all(&app).unwrap();
    sh(&app, &["init", "-q", "-b", "main"]);
    std::fs::write(app.join("README.md"), "# app\n").unwrap();
    sh(&app, &["add", "."]);
    sh(
        &app,
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
    app.canonicalize().unwrap()
}

fn sh(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_owned()
}

fn request(repo: &Path, branch: &str) -> TaskRequest {
    TaskRequest {
        repo: repo.to_path_buf(),
        branch: branch.into(),
        base: None,
    }
}

#[test]
fn session_names_follow_tmux_agents() {
    assert_eq!(session_name("feat/search.v2"), "feat-search-v2");
    assert_eq!(session_name("fix: a b"), "fix--a-b");
}

#[test]
fn worktrees_go_beside_the_repo() {
    assert_eq!(
        worktree_path(Path::new("/src/app"), "feat/x"),
        PathBuf::from("/src/.worktrees/app-feat-x")
    );
}

#[test]
fn an_existing_worktree_is_found_but_never_the_main_checkout() {
    let list = "worktree /src/app\nHEAD 1\nbranch refs/heads/main\n\n\
                worktree /src/.worktrees/app-x\nHEAD 2\nbranch refs/heads/x\n";
    let main = Path::new("/src/app");
    assert_eq!(
        find_worktree(list, main, "x"),
        Some(PathBuf::from("/src/.worktrees/app-x"))
    );
    assert_eq!(find_worktree(list, main, "main"), None);
    assert_eq!(find_worktree(list, main, "y"), None);
}

#[tokio::test]
async fn requests_are_checked_at_the_boundary() {
    let app = repo("validate");
    let app_str = app.to_str().unwrap();
    assert!(validate(app_str, "feat/ok", Some("main")).await.is_ok());
    for (repo, branch) in [
        ("relative/path", "x"),
        ("/no/such/dir", "x"),
        (app_str, ""),
        (app_str, "--upload-pack=x"),
        (app_str, "a..b"),
        (app_str, "has space"),
    ] {
        assert!(
            matches!(
                validate(repo, branch, None).await,
                Err(TaskError::Invalid(_))
            ),
            "{repo} {branch} was accepted"
        );
    }
    assert!(matches!(
        validate(app_str, "ok", Some("-x")).await,
        Err(TaskError::Invalid(_))
    ));
    let outside = std::env::temp_dir();
    assert!(matches!(
        validate(outside.to_str().unwrap(), "x", None).await,
        Err(TaskError::Invalid(_))
    ));
}

#[tokio::test]
async fn a_new_branch_gets_a_worktree_and_a_second_launch_reuses_it() {
    let app = repo("worktree");
    let first = builtin::worktree(&request(&app, "feat/x"))
        .await
        .unwrap()
        .path;
    assert_eq!(first, worktree_path(&app, "feat/x"));
    assert_eq!(sh(&first, &["branch", "--show-current"]), "feat/x");
    // From inside the worktree, the main checkout is still the repo.
    assert_eq!(main_checkout(&first).await.unwrap(), app);

    let again = builtin::worktree(&request(&first, "feat/x")).await.unwrap();
    assert_eq!(again.path, first);
    assert_eq!(again.main, app);
}

#[tokio::test]
async fn the_main_checkout_branch_is_refused() {
    let app = repo("main");
    let err = builtin::worktree(&request(&app, "main")).await.unwrap_err();
    assert!(matches!(err, TaskError::Invalid(m) if m.contains("main checkout")));
}
