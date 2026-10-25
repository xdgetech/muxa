use std::process::Command;

use super::*;
use crate::agents::claude::ClaudeAdapter;

const SAMPLE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/data/claude-code");
const SESSION_ID: &str = "0b8c3c0e-1111-4a2b-9c3d-000000000003";

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

fn pane(path: &Path) -> PaneInfo {
    PaneInfo {
        pane: "%7".into(),
        session: "work".into(),
        // A live pid inside "the pane" for the Claude adapter's liveness check.
        pid: std::process::id(),
        path: path.join("docs").display().to_string(),
        shell_foreground: false,
    }
}

fn paths(listing: &Listing) -> Vec<&str> {
    listing.docs.iter().map(|d| d.info.path.as_str()).collect()
}

#[test]
fn lists_tracked_and_untracked_markdown_but_not_ignored_files() {
    let root = repo("list");
    let listing = list(&pane(&root), &Registry::new(vec![]));
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
            .all(|d| d.info.kind == DocKind::Repo && d.info.mtime.is_some())
    );
}

#[test]
fn reads_only_listed_documents_inside_their_directory() {
    let root = repo("read");
    let listing = list(&pane(&root), &Registry::new(vec![]));
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
        Err(ReadError::TooLarge("docs/plan.md".into()))
    );
}

#[test]
fn a_pane_outside_any_repo_lists_nothing() {
    let dir = temp("norepo");
    let listing = list(&pane(&dir), &Registry::new(vec![]));
    assert!(listing.docs.is_empty());
    assert_eq!(listing.root, None);
}

#[test]
fn the_claude_plan_comes_first_and_is_named_by_the_transcript() {
    let root = repo("plan");
    let claude = temp("plan-claude");
    let mut session: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(format!("{SAMPLE}/session.json")).unwrap())
            .unwrap();
    session["pid"] = std::process::id().into();
    session["sessionId"] = SESSION_ID.into();
    session["tmux"] = "work:@1.%7".into();
    write(
        &claude.join(format!("sessions/{}.json", std::process::id())),
        &session.to_string(),
    );
    let transcript = std::fs::read_to_string(format!("{SAMPLE}/transcript.jsonl")).unwrap();
    write(
        &claude.join(format!("projects/-work-app/{SESSION_ID}.jsonl")),
        &transcript,
    );
    write(
        &claude.join("plans/sample-plan.md"),
        "# Reader plan\n\nBody\n",
    );

    let agents = Registry::new(vec![Box::new(ClaudeAdapter::new(claude.clone()))]);
    let listing = list(&pane(&root), &agents);
    assert_eq!(paths(&listing)[0], "plans/sample-plan.md");
    let plan = &listing.docs[0];
    assert_eq!(plan.info.kind, DocKind::Plan);
    assert_eq!(plan.info.title.as_deref(), Some("Reader plan"));
    assert_eq!(read(plan).unwrap().0, "# Reader plan\n\nBody\n");

    // A slug that would leave the plans directory is not used.
    let evil = transcript.replace("\"slug\":\"sample-plan\"", "\"slug\":\"../../etc/passwd\"");
    write(
        &claude.join(format!("projects/-work-app/{SESSION_ID}.jsonl")),
        &evil,
    );
    assert_eq!(list(&pane(&root), &agents).docs[0].info.kind, DocKind::Repo);
}
