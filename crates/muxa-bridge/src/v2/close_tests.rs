use std::process::Command;

use super::*;
use crate::v2::git::BranchRequest;

fn git_in(dir: &Path, args: &[&str]) {
    let ok = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(["-c", "user.name=t", "-c", "user.email=t@t"])
        .args(args)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .status()
        .unwrap()
        .success();
    assert!(ok, "git {args:?}");
}

fn branch_exists(main: &Path, branch: &str) -> bool {
    Command::new("git")
        .arg("-C")
        .arg(main)
        .args([
            "show-ref",
            "--verify",
            "-q",
            &format!("refs/heads/{branch}"),
        ])
        .status()
        .unwrap()
        .success()
}

/// A repo `app` on `main` with one commit, a Task folder, and a worktree
/// on `fix` made the way a Task makes it.
struct Fixture {
    root: PathBuf,
    main: PathBuf,
    places: Vec<Place>,
}

impl Fixture {
    async fn new(name: &str) -> Fixture {
        let root = std::env::temp_dir().join(format!("muxa-close-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let main = root.join("app");
        std::fs::create_dir_all(&main).unwrap();
        let main = main.canonicalize().unwrap();
        git_in(&main, &["init", "-q", "-b", "main"]);
        std::fs::write(main.join("README.md"), "x\n").unwrap();
        git_in(&main, &["add", "."]);
        git_in(&main, &["commit", "-q", "-m", "init"]);
        let req = BranchRequest {
            repo: main.clone(),
            branch: "fix".into(),
            base: Some("main".into()),
        };
        let tree = git::worktree(&req).await.unwrap();
        let folder = root.join("tasks").join("fix-t-1");
        std::fs::create_dir_all(&folder).unwrap();
        let place = |kind, path: &Path, branch: Option<&str>| Place {
            kind,
            path: path.to_string_lossy().into_owned(),
            branch: branch.map(str::to_owned),
        };
        let places = vec![
            place(PlaceKind::Folder, &folder, None),
            place(PlaceKind::Branch, &tree.path, Some("fix")),
            place(PlaceKind::Read, &main, None),
        ];
        Fixture { root, main, places }
    }

    fn tree(&self) -> PathBuf {
        PathBuf::from(&self.places[1].path)
    }

    fn kept_folder_and_read_place(&self) {
        assert!(Path::new(&self.places[0].path).is_dir(), "the folder stays");
        assert!(
            self.main.join("README.md").is_file(),
            "the read place stays"
        );
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

#[tokio::test]
async fn a_clean_merged_worktree_goes_with_its_branch() {
    let f = Fixture::new("clean").await;
    check(&f.places, WorktreeMode::Remove).await.unwrap();
    remove(&f.places, WorktreeMode::Remove).await.unwrap();
    assert!(!f.tree().exists());
    assert!(!branch_exists(&f.main, "fix"));
    f.kept_folder_and_read_place();
    // A second close finds nothing left to remove.
    remove(&f.places, WorktreeMode::Remove).await.unwrap();
}

#[tokio::test]
async fn uncommitted_changes_would_be_lost_until_forced() {
    let f = Fixture::new("dirty").await;
    std::fs::write(f.tree().join("notes.txt"), "draft\n").unwrap();
    // `.muxa/` is muxa's own and never counts as work.
    git::write_file(&f.tree(), "TASK.md", "# fix\n").unwrap();
    git::exclude(&f.tree()).unwrap();
    let Err(TaskError::WouldLoseWork(why)) = check(&f.places, WorktreeMode::Remove).await else {
        panic!("a dirty worktree must not close");
    };
    assert!(why.contains("fix has uncommitted changes"), "{why}");
    assert!(!why.contains("not in"), "{why}");
    assert!(f.tree().join("notes.txt").is_file(), "nothing changed");

    check(&f.places, WorktreeMode::Force).await.unwrap();
    remove(&f.places, WorktreeMode::Force).await.unwrap();
    assert!(!f.tree().exists());
    assert!(!branch_exists(&f.main, "fix"));
    f.kept_folder_and_read_place();
}

#[tokio::test]
async fn unmerged_commits_would_be_lost_until_forced() {
    let f = Fixture::new("ahead").await;
    std::fs::write(f.tree().join("fix.txt"), "fixed\n").unwrap();
    git_in(&f.tree(), &["add", "fix.txt"]);
    git_in(&f.tree(), &["commit", "-q", "-m", "fix"]);
    let Err(TaskError::WouldLoseWork(why)) = check(&f.places, WorktreeMode::Remove).await else {
        panic!("an unmerged branch must not close");
    };
    assert!(
        why.contains("fix has commits that are not in main"),
        "{why}"
    );
    assert!(branch_exists(&f.main, "fix"), "nothing changed");

    remove(&f.places, WorktreeMode::Force).await.unwrap();
    assert!(!f.tree().exists());
    assert!(!branch_exists(&f.main, "fix"));
}

#[tokio::test]
async fn a_merged_branch_closes_without_force() {
    let f = Fixture::new("merged").await;
    std::fs::write(f.tree().join("fix.txt"), "fixed\n").unwrap();
    git_in(&f.tree(), &["add", "fix.txt"]);
    git_in(&f.tree(), &["commit", "-q", "-m", "fix"]);
    git_in(&f.main, &["merge", "-q", "--ff-only", "fix"]);
    check(&f.places, WorktreeMode::Remove).await.unwrap();
    remove(&f.places, WorktreeMode::Remove).await.unwrap();
    assert!(!branch_exists(&f.main, "fix"));
}

#[tokio::test]
async fn a_main_checkout_is_never_removed() {
    let f = Fixture::new("main").await;
    let wrong = vec![Place {
        kind: PlaceKind::Branch,
        path: f.main.to_string_lossy().into_owned(),
        branch: Some("main".into()),
    }];
    remove(&wrong, WorktreeMode::Force).await.unwrap();
    assert!(f.main.join("README.md").is_file());
    assert!(branch_exists(&f.main, "main"));
}
