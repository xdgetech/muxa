use std::process::Command;

use super::*;

fn temp(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("muxa-docs-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir.canonicalize().unwrap()
}

fn write(path: &Path, text: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

fn git(dir: &Path, args: &[&str]) {
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
}

/// A repo with a tracked README, an untracked plan, an ignored secret, a non-Markdown
/// file and a symlink that points out of the repo.
fn repo(name: &str) -> PathBuf {
    let dir = temp(name).join("app");
    std::fs::create_dir_all(&dir).unwrap();
    git(&dir, &["init", "-q"]);
    write(&dir.join("README.md"), "# App\n");
    write(&dir.join(".gitignore"), "secret.md\n");
    git(&dir, &["add", "README.md", ".gitignore"]);
    write(
        &dir.join("docs/plan.md"),
        "# Plan\n\n```mermaid\nflowchart LR\n  A --> B\n```\n",
    );
    write(&dir.join("secret.md"), "token\n");
    write(&dir.join("notes.txt"), "not markdown\n");
    let outside = dir.parent().unwrap().join("outside.md");
    write(&outside, "outside\n");
    std::os::unix::fs::symlink(&outside, dir.join("docs/escape.md")).unwrap();
    dir
}

fn paths(listing: &Listing) -> Vec<&str> {
    listing.docs.iter().map(|d| d.info.path.as_str()).collect()
}

#[test]
fn lists_tracked_and_untracked_markdown() {
    let root = repo("list");
    let listing = for_folder(&root, false);
    assert_eq!(listing.root.as_deref(), Some("app"));
    assert_eq!(
        paths(&listing),
        ["README.md", "docs/escape.md", "docs/plan.md"]
    );
    assert!(!listing.truncated);
    assert!(
        listing
            .docs
            .iter()
            .all(|d| d.info.kind == DocKind::Repo && d.info.mtime.is_some() && !d.ignored)
    );
}

#[test]
fn ignored_documents_are_listed_except_vendored_dot_and_secret_ones() {
    let root = repo("ignored");
    write(
        &root.join(".gitignore"),
        "secret.md\noutput/\nnode_modules/\ntarget/\n.cache/\nlocal/\n",
    );
    let report = "output/qa/WBB-2317/20261124-142321/report.md";
    write(&root.join(report), "# QA report\n");
    write(&root.join("output/qa/index.html"), "<title>QA</title>");
    for skipped in [
        "node_modules/pkg/README.md",
        "target/doc/notes.md",
        "output/node_modules/pkg/README.md",
        ".cache/notes.md",
        "output/.hidden/notes.md",
        "output/.env.md",
        "local/API_TOKENS.md",
        "local/credentials.md",
    ] {
        write(&root.join(skipped), "token\n");
    }

    let listing = for_folder(&root, false);
    assert_eq!(
        paths(&listing),
        ["README.md", "docs/escape.md", "docs/plan.md", report]
    );
    let doc = listing.find(report).unwrap();
    assert!(doc.ignored);
    assert_eq!(read(doc).unwrap().0, "# QA report\n");
    assert!(!listing.find("README.md").unwrap().ignored);

    let asked = for_folder(&root, true);
    assert!(asked.find("output/qa/index.html").unwrap().ignored);
}

#[test]
fn tracked_documents_come_before_ignored_ones_at_the_limit() {
    let root = repo("limit");
    write(&root.join(".gitignore"), "secret.md\nout/\n");
    write(&root.join("out/a.md"), "a\n");
    write(&root.join("out/b.md"), "b\n");
    let (files, truncated) = repo_files(&root, false, 4);
    assert_eq!(
        files,
        [
            ("README.md".to_owned(), false),
            ("docs/escape.md".to_owned(), false),
            ("docs/plan.md".to_owned(), false),
            ("out/a.md".to_owned(), true),
        ]
    );
    assert!(truncated);
    let (files, truncated) = repo_files(&root, false, 2);
    assert_eq!(
        files,
        [
            ("README.md".to_owned(), false),
            ("docs/escape.md".to_owned(), false)
        ]
    );
    assert!(truncated);
}

#[test]
fn html_files_are_listed_and_opened_only_when_asked() {
    let root = repo("html");
    write(
        &root.join("build/report.html"),
        "<!doctype html><html><head><TITLE>\n  Weekly   report </title></head><body><script>1</script></body></html>",
    );
    write(&root.join("old.htm"), "<p>no title</p>");
    let outside = root.parent().unwrap().join("outside.html");
    write(&outside, "<title>Private</title>");
    std::os::unix::fs::symlink(&outside, root.join("escape.html")).unwrap();
    let plain = for_folder(&root, false);
    assert_eq!(
        plain.find("build/report.html"),
        Err(ReadError::NotListed("build/report.html".into()))
    );

    let asked = for_folder(&root, true);
    assert_eq!(
        paths(&asked),
        [
            "README.md",
            "build/report.html",
            "docs/escape.md",
            "docs/plan.md",
            "escape.html",
            "old.htm"
        ]
    );
    // Listed like any file, but nothing of it leaves the repo.
    let escape = asked.find("escape.html").unwrap();
    assert_eq!(escape.info.title, None);
    assert_eq!(read(escape), Err(ReadError::Outside("escape.html".into())));
    let report = asked.find("build/report.html").unwrap();
    assert_eq!(report.info.title.as_deref(), Some("Weekly report"));
    assert_eq!(asked.find("old.htm").unwrap().info.title, None);
    assert_eq!(asked.find("README.md").unwrap().info.title, None);
    assert!(read(report).unwrap().0.contains("<script>1</script>"));
}

#[test]
fn reads_only_listed_documents_inside_their_directory() {
    let root = repo("read");
    let listing = for_folder(&root, false);
    let (text, mtime) = read(listing.find("docs/plan.md").unwrap()).unwrap();
    assert!(text.contains("```mermaid"));
    assert!(mtime.is_some());

    assert_eq!(
        listing.find("secret.md"),
        Err(ReadError::NotListed("secret.md".into()))
    );
    assert_eq!(
        listing.find("../outside.md"),
        Err(ReadError::NotListed("../outside.md".into()))
    );
    assert_eq!(
        read(listing.find("docs/escape.md").unwrap()),
        Err(ReadError::Outside("docs/escape.md".into()))
    );

    write(
        &root.join("docs/plan.md"),
        &"x".repeat(MAX_DOC as usize + 1),
    );
    assert_eq!(
        read(listing.find("docs/plan.md").unwrap()),
        Err(ReadError::TooLarge("docs/plan.md".into(), MAX_DOC))
    );
}

#[test]
fn a_task_brief_is_listed_in_a_repo_and_without_one() {
    let root = repo("brief");
    write(&root.join(".git/info/exclude"), ".muxa/\n");
    write(&root.join(".muxa/TASK.md"), "# Ship it\n");
    write(&root.join(".muxa/HANDOFF.md"), "# Handoff\n");
    write(&root.join(".muxa/CHECK.md"), "# Check\n");
    let listing = for_folder(&root, false);
    assert_eq!(
        paths(&listing)[paths(&listing).len() - 3..],
        [".muxa/TASK.md", ".muxa/HANDOFF.md", ".muxa/CHECK.md"]
    );
    let brief = listing.find(".muxa/TASK.md").unwrap();
    assert_eq!(brief.info.title.as_deref(), Some("Ship it"));
    assert_eq!(read(brief).unwrap().0, "# Ship it\n");

    let dir = temp("brief-norepo");
    write(&dir.join(".muxa/TASK.md"), "# Taxes\n");
    let listing = for_folder(&dir, false);
    assert_eq!(paths(&listing), [".muxa/TASK.md"]);
    assert_eq!(listing.root, None);
}

#[test]
fn an_empty_folder_outside_any_repo_lists_nothing() {
    let dir = temp("norepo");
    let listing = for_folder(&dir, false);
    assert!(listing.docs.is_empty());
    assert_eq!(listing.root, None);
}
