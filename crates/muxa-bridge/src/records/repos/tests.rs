use std::path::{Path, PathBuf};

use super::*;

fn temp(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("muxa-repos-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir.canonicalize().unwrap()
}

/// A folder with a `.git` directory: enough for discovery.
fn repo(root: &Path, rel: &str) -> PathBuf {
    let dir = root.join(rel);
    std::fs::create_dir_all(dir.join(".git")).unwrap();
    dir
}

fn workspace(root: &Path, repos: Vec<PathBuf>) -> Workspace {
    Workspace {
        name: "lisk".into(),
        root: root.to_owned(),
        repos,
        agent: None,
        loop_checks: None,
        park_after: None,
        routines: None,
        assistant: None,
        role: Default::default(),
        memory: muxa_protocol::MemoryMode::Review,
    }
}

/// An Entity folder: repos one and two levels down, and everything that is
/// not one of its repos.
fn entity(name: &str) -> PathBuf {
    let root = temp(name);
    repo(&root, "app");
    repo(&root, "group/plan");
    repo(&root, "group/deeper/too-deep");
    repo(&root, ".worktrees/app-feat-x");
    repo(&root, ".hidden/repo");
    repo(&root, "tasks/fix-t-000001");
    repo(&root, "routines/daily-t-000002");
    repo(&root, "assistant/brief-t-000003");
    repo(&root, "app/nested");
    // A worktree has a `.git` file, not a directory.
    std::fs::create_dir_all(root.join("worktree")).unwrap();
    std::fs::write(root.join("worktree/.git"), "gitdir: /elsewhere\n").unwrap();
    std::fs::create_dir_all(root.join("notes")).unwrap();
    root
}

#[test]
fn discovery_finds_repos_two_levels_down_and_skips_the_rest() {
    let root = entity("discover");
    assert_eq!(
        discover(&root),
        [root.join("app"), root.join("group/plan")],
        "no dot-folder, tasks/, routines/, assistant/, worktree, inner repo or third level"
    );
    for (rel, found) in [
        ("app", true),
        ("group/plan", true),
        ("group", false),
        ("group/deeper/too-deep", false),
        (".worktrees/app-feat-x", false),
        (".hidden/repo", false),
        ("tasks/fix-t-000001", false),
        ("routines/daily-t-000002", false),
        ("assistant/brief-t-000003", false),
        ("app/nested", false),
        ("worktree", false),
        ("notes", false),
    ] {
        assert_eq!(under_root(&root, &root.join(rel)), found, "{rel}");
    }
    assert!(!under_root(&root, &root));
    assert!(!under_root(&root, Path::new("/elsewhere/app")));
}

#[test]
fn discovery_never_follows_a_symlink() {
    let root = temp("symlink");
    let outside = temp("symlink-outside");
    repo(&outside, "secret");
    std::os::unix::fs::symlink(&outside, root.join("link")).unwrap();
    std::os::unix::fs::symlink(outside.join("secret"), root.join("secret")).unwrap();
    assert!(discover(&root).is_empty());
    assert!(!under_root(&root, &root.join("secret")));
}

#[test]
fn discovery_stops_at_its_entry_limit() {
    let root = temp("cap");
    let total = MAX_ENTRIES + 50;
    for i in 0..total {
        repo(&root, &format!("r{i:05}"));
    }
    let start = Instant::now();
    let found = discover(&root);
    assert!(found.len() <= MAX_ENTRIES, "{} repos", found.len());
    assert!(start.elapsed() < MAX_SCAN * 4);
}

#[test]
fn the_repo_list_merges_the_root_and_the_listed_repos() {
    let root = entity("merge");
    let elsewhere = temp("merge-elsewhere");
    let shared = repo(&elsewhere, "shared");
    let ws = workspace(&root, vec![shared.clone(), root.join("app")]);
    assert_eq!(
        ws.repo_list(),
        [root.join("app"), root.join("group/plan"), shared.clone()]
    );
    assert!(ws.has_repo(&shared));
    assert!(ws.has_repo(&root.join("group/plan")));
    assert!(!ws.has_repo(&root.join("notes")));
}

#[test]
fn a_relative_repo_resolves_under_the_root_and_cannot_escape_it() {
    let root = entity("resolve");
    let elsewhere = temp("resolve-elsewhere");
    let shared = repo(&elsewhere, "shared");
    let secret = repo(&elsewhere, "secret");
    std::os::unix::fs::symlink(&secret, root.join("escape")).unwrap();
    std::os::unix::fs::symlink(&shared, root.join("linked")).unwrap();
    let ws = workspace(&root, vec![shared.clone()]);

    assert_eq!(ws.repo_path("app"), Ok(root.join("app")));
    assert_eq!(ws.repo_path("group/plan"), Ok(root.join("group/plan")));
    assert_eq!(ws.repo_path("app/"), Ok(root.join("app")));
    assert_eq!(
        ws.repo_path("/abs/as/given"),
        Ok(PathBuf::from("/abs/as/given"))
    );
    // A symlink may reach a listed repo, nothing else outside.
    assert_eq!(ws.repo_path("linked"), Ok(shared.clone()));
    assert!(matches!(
        ws.repo_path("escape"),
        Err(RepoError::Outside { .. })
    ));
    assert!(matches!(
        ws.repo_path(&format!(
            "../{}/secret",
            elsewhere.file_name().unwrap().to_str().unwrap()
        )),
        Err(RepoError::Outside { .. })
    ));
    // A listed repo outside the root by its folder name.
    assert_eq!(ws.repo_path("shared"), Ok(shared));
    assert!(matches!(
        ws.repo_path("nope"),
        Err(RepoError::NotFound { .. })
    ));
    assert_eq!(ws.repo_path(" "), Err(RepoError::Empty));
}

#[test]
fn a_folder_name_shared_by_two_listed_repos_is_ambiguous() {
    let root = temp("ambiguous");
    let a = repo(&temp("ambiguous-a"), "app");
    let b = repo(&temp("ambiguous-b"), "app");
    let ws = workspace(&root, vec![a, b]);
    assert!(matches!(
        ws.repo_path("app"),
        Err(RepoError::NotFound { .. })
    ));
}
