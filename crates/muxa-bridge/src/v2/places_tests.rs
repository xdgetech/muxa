use std::path::{Path, PathBuf};
use std::process::Command;

use super::*;

fn temp(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("muxa-places-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir.canonicalize().unwrap()
}

fn git(dir: &Path, args: &[&str]) {
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

/// A Workspace root with two repos, `app` and `docs`, each with one commit.
fn workspace(name: &str) -> Workspace {
    let root = temp(name);
    for repo in ["app", "docs"] {
        let dir = root.join(repo);
        std::fs::create_dir_all(&dir).unwrap();
        git(&dir, &["init", "-q", "-b", "main"]);
        std::fs::write(dir.join("README.md"), "x\n").unwrap();
        git(&dir, &["add", "."]);
        git(
            &dir,
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
    Workspace {
        name: "w".into(),
        root: root.clone(),
        repos: Vec::new(),
        missing_repos: Vec::new(),
        agent: None,
        loop_checks: None,
        park_after: None,
        routines: None,
        connections: Vec::new(),
        mcp: Vec::new(),
    }
}

fn branch(repo: &str, name: &str) -> PlaceSpec {
    PlaceSpec::Branch {
        repo: repo.into(),
        branch: name.into(),
        base: None,
    }
}

fn read(repo: &str) -> PlaceSpec {
    PlaceSpec::Read { repo: repo.into() }
}

#[tokio::test]
async fn no_places_is_one_folder_in_the_tasks_folder() {
    let ws = workspace("folder");
    let made = make(&ws, "Budget 2027", check(&ws, &[]).await.unwrap())
        .await
        .unwrap();
    assert_eq!(made.places.len(), 1);
    assert_eq!(made.places[0].kind, PlaceKind::Folder);
    let working = made.working();
    assert!(working.is_dir());
    assert!(working.starts_with(ws.root.join("tasks")));
    let name = working.file_name().unwrap().to_string_lossy();
    assert!(name.starts_with("budget-2027-t-"), "{name}");
    assert_eq!((made.repo, made.branch), (None, None));
}

#[tokio::test]
async fn only_read_places_get_a_folder_to_work_in() {
    let ws = workspace("audit");
    let made = make(
        &ws,
        "Audit",
        check(&ws, &[read("app"), read("docs")]).await.unwrap(),
    )
    .await
    .unwrap();
    let kinds: Vec<PlaceKind> = made.places.iter().map(|p| p.kind).collect();
    assert_eq!(kinds, [PlaceKind::Folder, PlaceKind::Read, PlaceKind::Read]);
    assert_eq!(made.places[1].path, ws.root.join("app").to_string_lossy());
}

#[tokio::test]
async fn a_branch_is_the_working_place_without_a_folder() {
    let ws = workspace("branch");
    let made = make(
        &ws,
        "Fix",
        check(&ws, &[read("docs"), branch("app", "fix")])
            .await
            .unwrap(),
    )
    .await
    .unwrap();
    assert_eq!(made.places[0].kind, PlaceKind::Branch);
    assert_eq!(made.places[0].branch.as_deref(), Some("fix"));
    assert_eq!(made.places[1].kind, PlaceKind::Read);
    assert_eq!(made.repo.as_deref(), Some(ws.root.join("app").as_path()));
    assert_eq!(made.branch.as_deref(), Some("fix"));
    assert!(made.working().join("README.md").is_file(), "a worktree");
}

#[tokio::test]
async fn a_repo_twice_two_folders_and_an_unknown_repo_are_refused() {
    let ws = workspace("refused");
    for bad in [
        vec![read("app"), branch("app", "fix")],
        vec![PlaceSpec::Folder, PlaceSpec::Folder],
        vec![read("nope")],
        vec![branch("app", "-x")],
    ] {
        assert!(
            matches!(check(&ws, &bad).await, Err(TaskError::Invalid(_))),
            "{bad:?}"
        );
    }
}

#[test]
fn task_md_lists_each_place_with_its_mode() {
    let made = Made {
        places: vec![
            Place {
                kind: PlaceKind::Folder,
                path: "/w/tasks/a-t-000001".into(),
                branch: None,
            },
            Place {
                kind: PlaceKind::Branch,
                path: "/w/app-fix".into(),
                branch: Some("fix".into()),
            },
            Place {
                kind: PlaceKind::Read,
                path: "/w/docs".into(),
                branch: None,
            },
        ],
        repo: None,
        branch: None,
    };
    let text = task_text(&made, "Audit", Some("Find the gaps."));
    assert!(text.starts_with("# Audit\n\n## Goal\n\nFind the gaps.\n\n## Places\n\n"));
    assert!(text.contains(
        "- Working place, you start here: /w/tasks/a-t-000001 (a folder for this Task)\n"
    ));
    assert!(text.contains("- /w/app-fix (a worktree on branch fix)\n"));
    assert!(text.contains("- /w/docs (read only: do not change anything here)\n"));
}

#[test]
fn claude_gets_the_other_places_and_no_edits_in_read_ones() {
    let others = [
        Place {
            kind: PlaceKind::Branch,
            path: "/w/app-fix".into(),
            branch: Some("fix".into()),
        },
        Place {
            kind: PlaceKind::Read,
            path: "/w/my docs/".into(),
            branch: None,
        },
    ];
    let v: serde_json::Value = serde_json::from_str(&settings_text(&others)).unwrap();
    assert_eq!(
        v,
        serde_json::json!({"permissions": {
            "additionalDirectories": ["/w/app-fix", "/w/my docs/"],
            "deny": ["Edit(//w/my docs/**)"],
        }})
    );
}

#[test]
fn a_one_place_task_writes_no_settings_and_paths_are_quoted() {
    let made = Made {
        places: vec![Place {
            kind: PlaceKind::Folder,
            path: "/x".into(),
            branch: None,
        }],
        repo: None,
        branch: None,
    };
    assert_eq!(claude_settings(&made).unwrap(), None);
    assert_eq!(quote("/a b/it's"), "'/a b/it'\\''s'");
}
