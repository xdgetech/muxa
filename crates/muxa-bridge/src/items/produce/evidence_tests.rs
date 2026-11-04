use std::path::PathBuf;
use std::process::Command;
use std::time::{Duration, UNIX_EPOCH};

use super::*;

struct Dir(PathBuf);

impl Drop for Dir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn dir() -> Dir {
    let d = std::env::temp_dir().join(format!(
        "muxa-evidence-{}-{:08x}",
        std::process::id(),
        getrandom::u32().unwrap()
    ));
    std::fs::create_dir_all(&d).unwrap();
    Dir(d.canonicalize().unwrap())
}

/// Writes `rel` with its mtime at `at`, epoch seconds.
fn write(root: &Path, rel: &str, at: u64) {
    let path = root.join(rel);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, "x").unwrap();
    std::fs::File::options()
        .write(true)
        .open(&path)
        .unwrap()
        .set_modified(UNIX_EPOCH + Duration::from_secs(at))
        .unwrap();
}

const START: u64 = 1_700_000_000;

#[test]
fn outside_git_the_turn_attaches_what_it_changed() {
    let d = dir();
    write(&d.0, "old.md", START - 10);
    write(&d.0, "report.md", START + 5);
    write(&d.0, "out/chart.png", START + 9);
    write(&d.0, ".muxa/HANDOFF.md", START + 5);
    write(&d.0, ".claude/plan.md", START + 5);
    write(&d.0, "notes.txt", START + 5);
    let a = attachments(&d.0, None, Some(START));
    assert_eq!(a.docs, ["report.md"]);
    assert_eq!(a.images, ["out/chart.png"]);
    assert_eq!(a.changes, None);
    assert_eq!(attachments(&d.0, None, None), Attachments::default());
}

#[test]
fn outside_git_the_newest_twenty_are_kept() {
    let d = dir();
    for n in 0..30 {
        write(&d.0, &format!("d{n:02}.md"), START + n);
    }
    let a = attachments(&d.0, None, Some(START));
    assert_eq!(a.docs.len(), MAX_PATHS);
    assert_eq!(a.docs[0], "d29.md");
    assert!(!a.docs.contains(&"d09.md".to_owned()));
}

#[test]
fn the_summary_names_files_inside_the_folder_only() {
    let d = dir();
    let outside = dir();
    write(&d.0, "docs/design.md", START - 100);
    write(&d.0, "shot.png", START - 100);
    write(&d.0, ".muxa/TASK.md", START - 100);
    write(&outside.0, "secret.md", START);
    std::os::unix::fs::symlink(outside.0.join("secret.md"), d.0.join("link.md")).unwrap();
    let summary = format!(
        "Wrote `docs/design.md` and {}/shot.png. Also {}/secret.md, link.md, \
         missing.md, ../x.md and .muxa/TASK.md. PR: https://github.com/e/a/pull/9.",
        d.0.display(),
        outside.0.display()
    );
    let a = attachments(&d.0, Some(&summary), Some(START));
    assert_eq!(a.docs, ["docs/design.md"]);
    assert_eq!(a.images, ["shot.png"]);
    assert_eq!(a.links.len(), 1);
    assert_eq!(a.links[0].title.as_deref(), Some("PR #9"));
}

#[test]
fn in_a_repo_the_summary_adds_to_the_changes_once() {
    let d = dir();
    let git = |args: &[&str]| {
        let ok = Command::new("git")
            .arg("-C")
            .arg(&d.0)
            .args(args)
            .output()
            .unwrap()
            .status
            .success();
        assert!(ok, "git {args:?}");
    };
    git(&["init", "-q"]);
    write(&d.0, "app/README.md", START);
    write(&d.0, "app/docs/old.md", START);
    git(&["add", "."]);
    git(&[
        "-c",
        "user.email=t@example.com",
        "-c",
        "user.name=t",
        "commit",
        "-qm",
        "init",
    ]);
    std::fs::write(d.0.join("app/README.md"), "changed").unwrap();
    let pane = d.0.join("app");
    let a = attachments(&pane, Some("See README.md and docs/old.md."), Some(START));
    assert_eq!(a.docs, ["app/README.md", "app/docs/old.md"], "repo paths");
    assert_eq!(a.changes.map(|c| c.files), Some(1));
}
