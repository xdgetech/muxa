use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, SystemTime};

use super::*;
use crate::records::{FORMAT_VERSION, RunReason, Seen};

/// A temp directory, removed on drop.
struct Temp(PathBuf);

impl Temp {
    fn new(name: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("muxa-facts-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        Self(dir.canonicalize().unwrap())
    }

    fn write(&self, rel: &str, age: Duration) {
        let path = self.0.join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "x").unwrap();
        let file = std::fs::File::options().write(true).open(&path).unwrap();
        file.set_modified(SystemTime::now() - age).unwrap();
    }
}

impl Drop for Temp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn record() -> Record {
    Record {
        version: FORMAT_VERSION,
        id: "t-7f3a2c".into(),
        rev: 4,
        workspace: "work".into(),
        title: "Fix login".into(),
        goal: "Secret goal text.".into(),
        status: Status::Review,
        created_at: 1_767_225_600,
        updated_at: 1_767_225_600,
        project: None,
        repo: None,
        branch: None,
        dir: None,
        session: None,
        note: Some("The start prompt was not sent.".into()),
        runs: Vec::new(),
        extra: serde_json::Map::new(),
    }
}

#[test]
fn the_handoff_lists_runs_and_facts_but_no_goal_or_transcript() {
    let mut r = record();
    r.start_run(RunReason::Start, 1_767_225_600);
    r.runs[0].fill(&Seen {
        agent: "claude-code".into(),
        session: Some("0b8c3c0e-1111".into()),
        transcript: Some("/h/.claude/projects/-w/0b8c3c0e-1111.jsonl".into()),
    });
    r.start_run(RunReason::Restart, 1_767_229_200);
    let work = Work::Files(Changed {
        since: 1_767_229_200,
        files: vec![("notes/plan.md".into(), 1_767_230_000)],
        more: true,
    });
    let text = render(&r, &r.runs, None, &work, 1_767_232_800);
    assert!(
        text.starts_with("# Handoff: Fix login\n\nmuxa wrote this file at 2026-01-01 02:00 UTC"),
        "{text}"
    );
    for line in [
        "- Status: review\n",
        "- Note: The start prompt was not sent.\n",
        "- Run 1 (start), 2026-01-01 00:00 UTC to 2026-01-01 01:00 UTC: claude-code. \
         Session 0b8c3c0e-1111. Transcript: /h/.claude/projects/-w/0b8c3c0e-1111.jsonl.\n",
        "- Run 2 (restart), 2026-01-01 01:00 UTC to end not recorded: agent not reported. \
         Session not reported.\n",
        "## Files changed since 2026-01-01 01:00 UTC\n\n- notes/plan.md (2026-01-01 01:13 UTC)\n",
        "only the newest 100 are shown",
    ] {
        assert!(text.contains(line), "missing {line:?} in\n{text}");
    }
    assert!(!text.contains("Secret goal"), "the goal is in TASK.md");
}

#[test]
fn an_agent_with_no_open_run_is_shown_on_its_own() {
    let seen = Seen {
        agent: "codex".into(),
        session: None,
        transcript: None,
    };
    let text = render(&record(), &[], Some(&seen), &Work::Unknown, 0);
    assert!(
        text.contains("- The agent replaced now: codex. Session not reported.\n"),
        "{text}"
    );
    assert!(!text.contains("No runs were recorded"));
    let text = render(&record(), &[], None, &Work::Unknown, 0);
    assert!(text.contains("No runs were recorded.\n"));
    assert!(text.contains("could not read the task's directory"));
}

#[test]
fn changed_files_skip_hidden_entries_and_older_files() {
    let dir = Temp::new("changed");
    dir.write("new.md", Duration::from_secs(10));
    dir.write("sub/deeper/newer.txt", Duration::from_secs(5));
    dir.write("old.md", Duration::from_secs(3600));
    dir.write(".muxa/TASK.md", Duration::ZERO);
    dir.write(".hidden/x", Duration::ZERO);
    std::os::unix::fs::symlink("/", dir.0.join("link")).unwrap();
    let since = crate::records::now() - 60;
    let c = changed_files(&dir.0, since);
    let names: Vec<&str> = c.files.iter().map(|(p, _)| p.as_str()).collect();
    assert_eq!(names, ["sub/deeper/newer.txt", "new.md"]);
    assert!(!c.more);
}

#[test]
fn changed_files_are_capped() {
    let dir = Temp::new("capped");
    for i in 0..(MAX_FILES + 3) {
        dir.write(&format!("f{i:03}"), Duration::ZERO);
    }
    let c = changed_files(&dir.0, 0);
    assert_eq!(c.files.len(), MAX_FILES);
    assert!(c.more);
}

fn run_git(dir: &Path, args: &[&str]) {
    let ok = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(["-c", "user.name=t", "-c", "user.email=t@t"])
        .args(args)
        .status()
        .unwrap()
        .success();
    assert!(ok, "git {args:?}");
}

#[test]
fn a_repo_task_gets_its_branch_commits_and_status() {
    let dir = Temp::new("repo");
    let repo = &dir.0;
    run_git(repo, &["init", "-q", "-b", "main"]);
    dir.write("a.txt", Duration::ZERO);
    run_git(repo, &["add", "."]);
    run_git(repo, &["commit", "-q", "-m", "init"]);
    run_git(repo, &["switch", "-q", "-c", "feat/x"]);
    dir.write("b.txt", Duration::ZERO);
    run_git(repo, &["add", "."]);
    run_git(repo, &["commit", "-q", "-m", "add b\u{1b}[31m"]);
    dir.write("c.txt", Duration::ZERO);

    let mut r = record();
    r.repo = Some(repo.clone());
    let Work::Git(g) = work(&r, repo, 0) else {
        panic!("not git")
    };
    assert_eq!(g.branch.as_deref(), Some("feat/x"));
    assert_eq!(g.base.as_deref(), Some("main"));
    assert_eq!(g.commits.len(), 1);
    assert!(g.commits[0].ends_with(" add b?[31m"), "{:?}", g.commits);
    assert_eq!(g.status, ["?? c.txt"]);
    let text = render(&r, &[], None, &Work::Git(g), 0);
    assert!(text.contains("- Commits since it left main: 1\n"), "{text}");
    assert!(text.contains("```\n?? c.txt\n```\n"), "{text}");
}
