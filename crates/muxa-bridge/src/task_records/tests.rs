use std::path::PathBuf;

use muxa_tmux::{Pane, PaneId, Role, Session, SessionId, WindowId};

use super::brief::{exclude_addition, merge, render};
use super::*;
use crate::records::FORMAT_VERSION;

fn record(id: &str, created_at: u64, status: Status, dir: Option<&str>) -> Record {
    Record {
        version: FORMAT_VERSION,
        id: id.into(),
        rev: 1,
        workspace: "work".into(),
        title: "Fix login".into(),
        goal: "Make login work.\n".into(),
        status,
        created_at,
        updated_at: created_at,
        project: None,
        repo: None,
        branch: None,
        dir: dir.map(PathBuf::from),
        session: None,
        note: None,
        runs: Vec::new(),
        extra: serde_json::Map::new(),
    }
}

fn session(n: u32, worktree: Option<&str>, path: &str, task_id: Option<&str>) -> Session {
    Session {
        id: SessionId::new(n),
        name: format!("s{n}"),
        repo: None,
        repo_dir: None,
        branch: None,
        worktree: worktree.map(str::to_owned),
        task: None,
        task_id: task_id.map(str::to_owned),
        ask: None,
        ask_root: false,
        panes: vec![Pane {
            id: PaneId::new(n),
            window: WindowId::new(n),
            index: 0,
            role: Role::Agent,
            cols: 80,
            rows: 24,
            command: "claude".into(),
            path: path.into(),
            dead: false,
            pid: 0,
            shell_foreground: false,
            agent: None,
        }],
    }
}

fn ids(sessions: &[Session]) -> Vec<Option<&str>> {
    sessions.iter().map(|s| s.task_id.as_deref()).collect()
}

#[test]
fn an_untagged_session_links_by_worktree_or_pane_path() {
    let records = [
        record("t-000001", 1, Status::Open, Some("/w/tasks/a-t-000001")),
        record("t-000002", 2, Status::Open, Some("/r/.worktrees/app-x")),
        record("t-000003", 3, Status::Open, None),
    ];
    let mut sessions = [
        session(1, None, "/w/tasks/a-t-000001", None),
        session(
            2,
            Some("/r/.worktrees/app-x"),
            "/r/.worktrees/app-x/src",
            None,
        ),
        session(3, None, "/elsewhere", None),
    ];
    link_records(&mut sessions, &records);
    assert_eq!(ids(&sessions), [Some("t-000001"), Some("t-000002"), None]);
}

#[test]
fn a_tag_wins_and_its_record_is_not_matched_again() {
    let records = [record("t-000001", 1, Status::Open, Some("/w/a"))];
    let mut sessions = [
        session(1, None, "/w/a", None),
        session(2, None, "/w/b", Some("t-000001")),
    ];
    link_records(&mut sessions, &records);
    assert_eq!(ids(&sessions), [None, Some("t-000001")]);

    // Two untagged sessions in one directory: only the first gets it.
    let mut sessions = [
        session(1, None, "/w/a", None),
        session(2, None, "/w/a", None),
    ];
    link_records(&mut sessions, &records);
    assert_eq!(ids(&sessions), [Some("t-000001"), None]);
}

#[test]
fn a_long_list_keeps_unfinished_tasks_then_the_newest() {
    let records = vec![
        record("t-000001", 1, Status::Open, None),
        record("t-000002", 2, Status::Done, None),
        record("t-000003", 3, Status::Dropped, None),
        record("t-000004", 4, Status::Done, None),
        record("t-000005", 5, Status::Review, None),
    ];
    let (kept, truncated) = cap(records.clone(), 3);
    assert!(truncated);
    let kept: Vec<&str> = kept.iter().map(|r| r.id.as_str()).collect();
    assert_eq!(kept, ["t-000001", "t-000004", "t-000005"]);
    let (all, truncated) = cap(records, 5);
    assert_eq!((all.len(), truncated), (5, false));
}

#[test]
fn task_md_holds_the_goal_and_where_the_work_is() {
    let mut r = record("t-7f3a2c", 1, Status::Open, None);
    r.project = Some("auth".into());
    r.repo = Some("/src/app".into());
    r.branch = Some("feat/login".into());
    let text = render(&r);
    // ADR 0015's part comes first, unchanged.
    assert!(text.starts_with(LEGACY), "{text}");
    // Then the request to the agent, the marker and its sections (ADR 0018).
    let notes = text.find(MARKER).expect("marker");
    assert!(text[..notes].contains("## How to use this file"));
    assert_eq!(
        &text[notes..],
        format!(
            "{MARKER}\n\n## Progress\n\nNothing yet.\n\n## Decisions\n\nNothing yet.\n\n## Next actions\n\nNothing yet.\n"
        )
    );
}

const LEGACY: &str = "# Fix login\n\n## Goal\n\nMake login work.\n\n## Context\n\n\
     - Task: t-7f3a2c\n- Workspace: work\n- Project: auth\n\
     - Repo: /src/app\n- Branch: feat/login\n";
const MARKER: &str = "<!-- muxa:notes - muxa never changes anything below this line. -->";

fn full_record() -> Record {
    let mut r = record("t-7f3a2c", 1, Status::Open, None);
    r.project = Some("auth".into());
    r.repo = Some("/src/app".into());
    r.branch = Some("feat/login".into());
    r
}

#[test]
fn a_rewrite_keeps_the_agents_notes_byte_for_byte() {
    let r = full_record();
    let notes = format!(
        "{MARKER}\n\n## Progress\n\n- Found the bug in `auth.rs`.\n\n## Decisions\n\n\
         - Kept the old API.\n\n## Next actions\n\n1. Add a test\n\n## Extra\n\nno trailing newline"
    );
    let old = format!("# An old title\n\nedited by hand\n\n{notes}");
    let new = merge(&r, Some(&old));
    assert!(new.starts_with(LEGACY), "{new}");
    assert!(new.ends_with(&notes), "{new}");
    assert_eq!(new.matches(MARKER).count(), 1);
    assert!(
        !new.contains("edited by hand"),
        "the header is the bridge's"
    );
    // Twice is the same as once.
    assert_eq!(merge(&r, Some(&new)), new);
    // A marker line with other words after its start still counts.
    let other = "<!-- muxa:notes, written by an older muxa -->\nmine\n";
    assert!(merge(&r, Some(other)).ends_with(other));
}

#[test]
fn a_file_without_the_marker_loses_nothing() {
    let r = full_record();
    // What ADR 0015 wrote is replaced.
    assert_eq!(merge(&r, Some(LEGACY)), render(&r));
    assert_eq!(merge(&r, None), render(&r));
    assert_eq!(merge(&r, Some("  \n")), render(&r));
    // Anything else is kept under "Earlier notes".
    let edited = format!("{LEGACY}\nMy own notes");
    let new = merge(&r, Some(&edited));
    assert!(new.starts_with(&render(&r)), "{new}");
    assert!(
        new.ends_with(&format!("## Earlier notes\n\n{edited}\n")),
        "{new}"
    );
}

#[test]
fn the_exclude_line_is_added_once() {
    assert_eq!(exclude_addition("").as_deref(), Some(".muxa/\n"));
    assert_eq!(
        exclude_addition("# git ls-files\n*.log").as_deref(),
        Some("\n.muxa/\n")
    );
    assert_eq!(exclude_addition("*.log\n.muxa/\n"), None);
    assert_eq!(exclude_addition("/.muxa/\n"), None);
}
