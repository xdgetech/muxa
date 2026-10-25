use std::path::{Path, PathBuf};
use std::process::Command;

use super::*;

/// A repo with two committed files, removed on drop.
struct Repo(PathBuf);

impl Repo {
    fn new(name: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("muxa-changes-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("src")).unwrap();
        let repo = Repo(dir.canonicalize().unwrap());
        repo.git(&["init", "-q", "-b", "main"]);
        repo.write("src/main.rs", "fn main() {\n    old();\n}\n");
        repo.write("gone.txt", "bye\n");
        repo.write("moved.txt", "one\ntwo\nthree\nfour\n");
        repo.git(&["add", "."]);
        repo.git(&[
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@t",
            "commit",
            "-q",
            "-m",
            "init",
        ]);
        repo
    }

    fn git(&self, args: &[&str]) {
        let ok = Command::new("git")
            .arg("-C")
            .arg(&self.0)
            .args(args)
            .status()
            .unwrap()
            .success();
        assert!(ok, "git {args:?}");
    }

    fn write(&self, rel: &str, text: impl AsRef<[u8]>) {
        std::fs::write(self.0.join(rel), text).unwrap();
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for Repo {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn summary(changes: &Changes) -> Vec<(String, ChangeStatus, Option<u32>, Option<u32>)> {
    changes
        .entries
        .iter()
        .map(|c| (c.path.clone(), c.status, c.added, c.removed))
        .collect()
}

#[test]
fn every_kind_of_change_is_listed_against_head() {
    let repo = Repo::new("list");
    repo.write("src/main.rs", "fn main() {\n    new();\n}\n");
    std::fs::remove_file(repo.path().join("gone.txt")).unwrap();
    repo.git(&["mv", "moved.txt", "renamed.txt"]);
    repo.write("staged.txt", "a\nb\n");
    repo.git(&["add", "staged.txt"]);
    repo.write("notes.md", "x\ny\nz\n");
    repo.write("logo.bin", [0u8, 1, 2, 0]);

    let changes = list(&repo.path().join("src"));
    assert_eq!(changes.root.as_deref(), Some(repo.path()));
    assert_eq!(
        summary(&changes),
        [
            ("gone.txt".into(), ChangeStatus::Deleted, Some(0), Some(1)),
            ("logo.bin".into(), ChangeStatus::Untracked, None, None),
            ("notes.md".into(), ChangeStatus::Untracked, Some(3), Some(0)),
            (
                "renamed.txt".into(),
                ChangeStatus::Renamed,
                Some(0),
                Some(0)
            ),
            (
                "src/main.rs".into(),
                ChangeStatus::Modified,
                Some(1),
                Some(1)
            ),
            ("staged.txt".into(), ChangeStatus::Added, Some(2), Some(0)),
        ]
    );
    assert_eq!(
        changes
            .find("renamed.txt")
            .and_then(|c| c.old_path.as_deref()),
        Some("moved.txt")
    );
}

#[test]
fn outside_a_repo_nothing_is_listed() {
    let changes = list(&std::env::temp_dir());
    assert!(changes.root.is_none());
    assert!(changes.entries.is_empty());
}

#[test]
fn diffs_are_unified_text_or_binary() {
    let repo = Repo::new("diff");
    repo.write("src/main.rs", "fn main() {\n    new();\n}\n");
    repo.write("notes.md", "hello\n");
    repo.write("logo.bin", [0u8, 1, 2, 0]);
    let changes = list(repo.path());
    let diff_of = |p: &str| diff(repo.path(), changes.find(p).unwrap());

    let text = diff_of("src/main.rs").unwrap().text.unwrap();
    assert!(text.contains("-    old();\n+    new();"), "{text}");
    let text = diff_of("notes.md").unwrap().text.unwrap();
    assert!(text.contains("+hello"), "{text}");
    assert_eq!(diff_of("logo.bin").unwrap().text, None);

    // Reverted: nothing to show any more.
    repo.write("src/main.rs", "fn main() {\n    old();\n}\n");
    assert_eq!(
        diff_of("src/main.rs"),
        Err(DiffError::Unchanged("src/main.rs".into()))
    );
}

#[test]
fn a_huge_diff_is_refused() {
    let repo = Repo::new("huge");
    repo.write("big.txt", "x".repeat(MAX_DIFF + 10));
    let changes = list(repo.path());
    assert_eq!(
        diff(repo.path(), changes.find("big.txt").unwrap()),
        Err(DiffError::TooLarge("big.txt".into()))
    );
}

#[test]
fn a_repo_without_commits_diffs_against_nothing() {
    let dir = std::env::temp_dir().join(format!("muxa-changes-{}-empty", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let repo = Repo(dir.canonicalize().unwrap());
    repo.git(&["init", "-q"]);
    repo.write("a.txt", "a\n");
    repo.git(&["add", "a.txt"]);
    let changes = list(repo.path());
    assert_eq!(
        summary(&changes),
        [("a.txt".into(), ChangeStatus::Added, Some(1), Some(0))]
    );
    assert!(
        diff(repo.path(), changes.find("a.txt").unwrap())
            .unwrap()
            .text
            .unwrap()
            .contains("+a")
    );
}

#[test]
fn status_and_numstat_parse_their_z_formats() {
    let status = b"1 .M N... 100644 100644 100644 a b src/a b.rs\x002 R. N... 100644 100644 100644 a b R100 new.rs\x00old.rs\x00? n.md\x00u UU N... 1 1 1 1 a b c x.rs\x00";
    let parsed: Vec<_> = parse_status(status)
        .into_iter()
        .map(|c| (c.path, c.status, c.old_path))
        .collect();
    assert_eq!(
        parsed,
        [
            ("src/a b.rs".into(), ChangeStatus::Modified, None),
            (
                "new.rs".into(),
                ChangeStatus::Renamed,
                Some("old.rs".into())
            ),
            ("n.md".into(), ChangeStatus::Untracked, None),
            ("x.rs".into(), ChangeStatus::Conflicted, None),
        ]
    );
    let counts =
        parse_numstat(b"1\t2\tsrc/a b.rs\x000\t0\t\x00old.rs\x00new.rs\x00-\t-\tlogo.png\x00");
    assert_eq!(counts["src/a b.rs"], (Some(1), Some(2)));
    assert_eq!(counts["new.rs"], (Some(0), Some(0)));
    assert_eq!(counts["logo.png"], (None, None));
}
