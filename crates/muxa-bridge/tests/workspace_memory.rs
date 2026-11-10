//! The workspace memory's copy (ADR 0053 slice M1), end to end: each Task
//! start and resume copies `<root>/.muxa/WORKSPACE.md` into the Task's
//! folder, a file a run may not read stops the start before any session,
//! and the lead's old `NOTES.md` is reported, never read.
#![cfg(feature = "tmux-integration")]

mod common;

use std::path::{Path, PathBuf};

use common::Server;
use common::memory::{Host, create, error_of, ok, saved, send, sessions, sha256};
use muxa_protocol::{MemoryMode, Message, TaskRecord};
use serde_json::json;

/// Memory text that must never reach the daemon's log (ADR 0053 §8).
const SENTINEL: &str = "memory-mark-5d21";

fn copy_in(r: &TaskRecord) -> PathBuf {
    Path::new(r.dir.as_deref().expect("a dir")).join(".muxa/WORKSPACE.md")
}

#[tokio::test]
async fn a_start_copies_the_file_and_a_resume_refreshes_it() {
    let server = Server::start();
    let mut host = Host::new("copy", "");
    let first = format!("# Lisk\n\n- {SENTINEL}: releases go out on Wednesday.\n");
    host.write_memory(&first);
    let mut bridge = host.bridge(&server, true).await;

    let (r, result) = create(&mut bridge, json!({})).await;
    ok(&result);
    let r = r.expect("task-saved");
    let copy = std::fs::read(copy_in(&r)).expect("the copy");
    assert_eq!(
        sha256(&copy),
        sha256(first.as_bytes()),
        "the live file's hash"
    );
    let task_md =
        std::fs::read_to_string(Path::new(r.dir.as_deref().unwrap()).join(".muxa/TASK.md"))
            .unwrap();
    assert!(task_md.contains("## Workspace memory"), "{task_md}");
    assert!(
        task_md.ends_with("## For the workspace\n\nNothing yet.\n"),
        "{task_md}"
    );

    // A resume is a restart: the copy follows the live file.
    let second = format!("{first}- {SENTINEL}: staging is eu-west.\n");
    host.write_memory(&second);
    let (_, result) = saved(&mut bridge, json!({"type": "park-task", "task": r.id})).await;
    ok(&result);
    let (_, result) = saved(&mut bridge, json!({"type": "restart-agent", "task": r.id})).await;
    ok(&result);
    assert_eq!(std::fs::read_to_string(copy_in(&r)).unwrap(), second);

    // Without a live file, the next run has no copy.
    std::fs::remove_file(host.memory()).unwrap();
    let (_, result) = saved(&mut bridge, json!({"type": "park-task", "task": r.id})).await;
    ok(&result);
    let (_, result) = saved(&mut bridge, json!({"type": "restart-agent", "task": r.id})).await;
    ok(&result);
    assert!(!copy_in(&r).exists());

    // task-list reports the mode and when the file changed.
    host.write_memory("# Lisk\n");
    let (before, result) = send(&mut bridge, json!({"type": "list-tasks"})).await;
    ok(&result);
    let workspaces = before
        .into_iter()
        .find_map(|m| match m {
            Message::TaskList { workspaces, .. } => Some(workspaces),
            _ => None,
        })
        .expect("task-list");
    assert_eq!(workspaces[0].memory, Some(MemoryMode::Auto));
    assert!(workspaces[0].memory_at.is_some());

    let log = host.log();
    assert!(log.contains("workspace memory copied"), "{log}");
    assert!(!log.contains(SENTINEL), "the text reached the log");
}

#[tokio::test]
async fn an_oversize_or_symlinked_file_fails_the_start_before_any_session() {
    let server = Server::start();
    let mut host = Host::new("refused", "");
    host.write_memory(&format!("{SENTINEL}{}", "x".repeat(16 << 10)));
    let mut bridge = host.bridge(&server, false).await;
    let before = sessions(&server);

    let (_, result) = create(&mut bridge, json!({"title": "Too big"})).await;
    let e = error_of(&result);
    assert!(
        e.message.contains(&host.memory().display().to_string()),
        "{e:?}"
    );
    assert!(e.message.contains("16400 bytes"), "{e:?}");
    assert!(!e.message.contains(SENTINEL), "{e:?}");
    assert_eq!(sessions(&server), before, "no session was started");

    std::fs::remove_file(host.memory()).unwrap();
    let elsewhere = host.dir.join("elsewhere.md");
    std::fs::write(&elsewhere, SENTINEL).unwrap();
    std::os::unix::fs::symlink(&elsewhere, host.memory()).unwrap();
    let (_, result) = create(&mut bridge, json!({"title": "Linked"})).await;
    let e = error_of(&result);
    assert!(e.message.contains("not a regular file"), "{e:?}");
    assert!(!e.message.contains(SENTINEL), "{e:?}");
    assert_eq!(sessions(&server), before, "no session was started");
}

#[tokio::test]
async fn the_old_notes_are_reported_and_not_read_and_a_members_stay() {
    let server = Server::start();
    let mut host = Host::new("old-notes", "");
    let old = host.root.join(".muxa/assistant/NOTES.md");
    let member = host
        .root
        .join(".muxa/assistant/staff/lisk-strategist/NOTES.md");
    std::fs::create_dir_all(member.parent().unwrap()).unwrap();
    std::fs::write(&old, SENTINEL).unwrap();
    std::fs::write(&member, "# Strategist\n").unwrap();
    let mut bridge = host.bridge(&server, false).await;

    let (before, result) = send(&mut bridge, json!({"type": "list-routines"})).await;
    ok(&result);
    let problems = before
        .into_iter()
        .find_map(|m| match m {
            Message::RoutineList { problems, .. } => Some(problems),
            _ => None,
        })
        .expect("routine-list");
    assert_eq!(problems.len(), 1, "{problems:?}");
    assert_eq!(problems[0].workspace, "lisk");
    assert!(
        problems[0]
            .reason
            .contains("move .muxa/assistant/NOTES.md to .muxa/WORKSPACE.md"),
        "{problems:?}"
    );

    let (r, result) = create(&mut bridge, json!({})).await;
    ok(&result);
    let r = r.expect("task-saved");
    assert!(!copy_in(&r).exists(), "the old notes are not the memory");
    assert_eq!(std::fs::read_to_string(&old).unwrap(), SENTINEL);
    assert_eq!(std::fs::read_to_string(&member).unwrap(), "# Strategist\n");
    let dir = Path::new(r.dir.as_deref().unwrap());
    assert!(
        !dir.join(".muxa/assistant").exists(),
        "a member's notes are not copied"
    );
}
