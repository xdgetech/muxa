//! `changes` and `diff` end to end, uncommitted and on a branch, for a pane whose working directory is a git repo.
#![cfg(feature = "tmux-integration")]

mod common;

use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;

use common::{Bridge, Server};
use muxa_protocol::{ChangeStatus, ErrorCode, Message};
use serde_json::json;

/// Longer than a few diff ticks.
const WAIT: Duration = Duration::from_secs(6);

struct Repo(PathBuf);

impl Repo {
    fn new(name: &str) -> Self {
        let dir =
            std::env::temp_dir().join(format!("muxa-changes-it-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let repo = Repo(dir.canonicalize().unwrap());
        repo.git(&["init", "-q", "-b", "main"]);
        std::fs::write(repo.0.join("a.txt"), "one\n").unwrap();
        repo.git(&["add", "."]);
        repo.git(&[
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@t",
            "commit",
            "-q",
            "-m",
            "i",
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
}

impl Drop for Repo {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

async fn next_diff(bridge: &mut Bridge) -> Message {
    loop {
        match bridge.event(WAIT).await.expect("no diff message") {
            msg @ (Message::DiffContent { .. } | Message::DiffEnded { .. }) => return msg,
            _ => continue,
        }
    }
}

#[tokio::test]
async fn lists_changes_and_follows_a_diff_until_it_is_reverted() {
    let server = Server::start();
    let repo = Repo::new("follow");
    std::fs::write(repo.0.join("a.txt"), "two\n").unwrap();
    std::fs::write(repo.0.join("new.md"), "# New\n").unwrap();
    let dir = repo.0.display().to_string();
    let pane = server.tmux(&[
        "new-window",
        "-d",
        "-P",
        "-F",
        "#{pane_id}",
        "-c",
        &dir,
        "sleep 600",
    ]);
    let (mut bridge, hello) = Bridge::start(&server, &[]).await;
    let Message::Hello { capabilities, .. } = hello else {
        panic!("{hello:?}")
    };
    assert!(capabilities.contains(&"changes".to_owned()));

    let result = bridge
        .request(json!({"type": "changes", "pane": pane}))
        .await;
    assert!(
        matches!(result, Message::Result { error: None, .. }),
        "{result:?}"
    );
    let Some(Message::ChangeList {
        changes, entries, ..
    }) = bridge.event(WAIT).await
    else {
        panic!("no change-list")
    };
    assert_eq!(changes, bridge.last_id());
    let listed: Vec<_> = entries
        .iter()
        .map(|e| (e.path.as_str(), e.status))
        .collect();
    assert_eq!(
        listed,
        [
            ("a.txt", ChangeStatus::Modified),
            ("new.md", ChangeStatus::Untracked)
        ]
    );

    let result = bridge
        .request(json!({"type": "diff", "pane": pane, "path": "a.txt"}))
        .await;
    assert!(
        matches!(result, Message::Result { error: None, .. }),
        "{result:?}"
    );
    let open = bridge.last_id();
    let Message::DiffContent {
        diff,
        text: Some(text),
        binary: false,
        ..
    } = next_diff(&mut bridge).await
    else {
        panic!("no text diff")
    };
    assert_eq!(diff, open);
    assert!(text.contains("-one\n+two"), "{text}");

    std::fs::write(repo.0.join("a.txt"), "three\n").unwrap();
    let Message::DiffContent {
        text: Some(text), ..
    } = next_diff(&mut bridge).await
    else {
        panic!("the change was not sent")
    };
    assert!(text.contains("+three"), "{text}");

    std::fs::write(repo.0.join("a.txt"), "one\n").unwrap();
    assert!(
        matches!(next_diff(&mut bridge).await, Message::DiffEnded { diff, .. } if diff == open)
    );
}

#[tokio::test]
async fn only_listed_files_can_be_diffed() {
    let server = Server::start();
    let repo = Repo::new("refuse");
    let dir = repo.0.display().to_string();
    let pane = server.tmux(&[
        "new-window",
        "-d",
        "-P",
        "-F",
        "#{pane_id}",
        "-c",
        &dir,
        "sleep 600",
    ]);
    let (mut bridge, _) = Bridge::start(&server, &[]).await;
    for path in ["a.txt", "../outside.txt", "/etc/hosts", "--output=/tmp/x"] {
        let Message::Result {
            error: Some(error), ..
        } = bridge
            .request(json!({"type": "diff", "pane": pane, "path": path}))
            .await
        else {
            panic!("{path} was diffed")
        };
        assert_eq!(error.code, ErrorCode::BadRequest, "{path}");
    }
}

#[tokio::test]
async fn a_branchs_commits_are_listed_and_diffed_against_main() {
    let server = Server::start();
    let repo = Repo::new("branch");
    repo.git(&["checkout", "-q", "-b", "feat"]);
    std::fs::write(repo.0.join("a.txt"), "two\n").unwrap();
    repo.git(&[
        "-c",
        "user.name=t",
        "-c",
        "user.email=t@t",
        "commit",
        "-q",
        "-am",
        "edit",
    ]);
    let dir = repo.0.display().to_string();
    let pane = server.tmux(&[
        "new-window",
        "-d",
        "-P",
        "-F",
        "#{pane_id}",
        "-c",
        &dir,
        "sleep 600",
    ]);
    let (mut bridge, hello) = Bridge::start(&server, &[]).await;
    let Message::Hello { capabilities, .. } = hello else {
        panic!("{hello:?}")
    };
    assert!(capabilities.contains(&"branch-changes".to_owned()));

    bridge
        .request(json!({"type": "changes", "pane": pane}))
        .await;
    let Some(Message::ChangeList {
        entries, branch, ..
    }) = bridge.event(WAIT).await
    else {
        panic!("no change-list")
    };
    assert!(entries.is_empty(), "nothing is uncommitted");
    let branch = branch.expect("a branch section");
    assert_eq!((branch.base.as_str(), branch.commits), ("main", 1));
    assert_eq!(branch.entries[0].path, "a.txt");

    // Committed, so only the branch list has it.
    let Message::Result {
        error: Some(error), ..
    } = bridge
        .request(json!({"type": "diff", "pane": pane, "path": "a.txt"}))
        .await
    else {
        panic!("an uncommitted diff of a committed file")
    };
    assert_eq!(error.code, ErrorCode::BadRequest);

    let result = bridge
        .request(json!({"type": "diff", "pane": pane, "path": "a.txt", "scope": "branch"}))
        .await;
    assert!(
        matches!(result, Message::Result { error: None, .. }),
        "{result:?}"
    );
    let Message::DiffContent {
        text: Some(text), ..
    } = next_diff(&mut bridge).await
    else {
        panic!("no branch diff")
    };
    assert!(text.contains("-one\n+two"), "{text}");
}
