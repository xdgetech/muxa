//! `new-task` end to end: the built-in launcher, and tmux-agents through a fake
//! `new.sh`. Neither may change an existing pane.
#![cfg(feature = "tmux-integration")]

mod common;

use std::path::Path;

use common::repo::{Repo, fake_new_sh, git};
use common::{Bridge, Server};
use muxa_protocol::{ErrorCode, Message};
use serde_json::json;

async fn new_task(bridge: &mut Bridge, repo: &Path, branch: &str) -> (Option<String>, Message) {
    let result = bridge
        .request(json!({"type": "new-task", "repo": repo, "branch": branch}))
        .await;
    let id = bridge.last_id();
    let mut created = None;
    while let Some(msg) = bridge.event(std::time::Duration::from_millis(200)).await {
        if let Message::TaskCreated { task, session } = msg {
            assert_eq!(task, id);
            created = Some(session);
        }
    }
    (created, result)
}

fn sessions(server: &Server) -> Vec<String> {
    server
        .tmux(&["list-sessions", "-F", "#{session_name}"])
        .lines()
        .map(str::to_owned)
        .collect()
}

#[tokio::test]
async fn the_builtin_launcher_makes_a_worktree_and_a_tagged_session() {
    let server = Server::start();
    let repo = Repo::new("builtin");
    let before = server.layout();
    let (mut bridge, hello) = Bridge::start(
        &server,
        &["--task-launcher", "builtin", "--task-command", "sleep 600"],
    )
    .await;
    let Message::Hello { capabilities, .. } = hello else {
        panic!("{hello:?}")
    };
    assert!(capabilities.contains(&"tasks".to_owned()));

    let (created, result) = new_task(&mut bridge, &repo.app, "feat/x").await;
    assert!(
        matches!(result, Message::Result { error: None, .. }),
        "{result:?}"
    );
    assert_eq!(created.as_deref(), Some("feat-x"));

    let worktree = repo.root.join("repos/.worktrees/app-feat-x");
    assert!(worktree.join("README.md").is_file());
    let pane = server.pane("=feat-x:");
    assert_eq!(
        server.tmux(&[
            "display-message",
            "-p",
            "-t",
            &pane,
            "#{@muxa_role} #{pane_current_path}"
        ]),
        format!("agent {}", worktree.display())
    );
    // The existing session's panes are exactly as they were.
    let after: Vec<String> = server
        .layout()
        .lines()
        .filter(|l| {
            before
                .lines()
                .any(|b| b.split(' ').nth(3) == l.split(' ').nth(3))
        })
        .map(str::to_owned)
        .collect();
    assert_eq!(after.join("\n"), before);

    // Again for the same branch: the same session, nothing new.
    let (created, _) = new_task(&mut bridge, &worktree, "feat/x").await;
    assert_eq!(created.as_deref(), Some("feat-x"));
    assert_eq!(sessions(&server).len(), 2);
}

#[tokio::test]
async fn tmux_agents_is_used_when_it_publishes_its_scripts() {
    let server = Server::start();
    let repo = Repo::new("agents");
    let scripts = repo.root.join("scripts");
    fake_new_sh(&scripts, false);
    server.tmux(&[
        "set-option",
        "-g",
        "@agents_scripts",
        scripts.to_str().unwrap(),
    ]);
    let (mut bridge, _) = Bridge::start(&server, &[]).await;

    let (created, result) = new_task(&mut bridge, &repo.app, "feat/x").await;
    assert!(
        matches!(result, Message::Result { error: None, .. }),
        "{result:?}"
    );
    assert_eq!(created.as_deref(), Some("fake-task"));
    assert!(sessions(&server).contains(&"fake-task".to_owned()));
    let args = std::fs::read_to_string(scripts.join("args")).unwrap();
    assert_eq!(
        args.lines().collect::<Vec<_>>(),
        [
            repo.app.to_str().unwrap(),
            "feat/x",
            "--no-switch",
            &format!("socket={}", server.socket),
        ]
    );
}

#[tokio::test]
async fn a_failing_launcher_reports_why() {
    let server = Server::start();
    let repo = Repo::new("fail");
    let script = fake_new_sh(&repo.root.join("scripts"), true);
    let flag = format!("tmux-agents={}", script.display());
    let (mut bridge, _) = Bridge::start(&server, &["--task-launcher", &flag]).await;

    let (created, result) = new_task(&mut bridge, &repo.app, "feat/x").await;
    assert_eq!(created, None);
    let Message::Result {
        error: Some(error), ..
    } = result
    else {
        panic!("{result:?}")
    };
    assert_eq!(error.code, ErrorCode::TaskFailed);
    assert_eq!(
        error.message,
        "could not create a worktree for feat/x: boom"
    );
}

#[tokio::test]
async fn bad_requests_start_nothing() {
    let server = Server::start();
    let repo = Repo::new("bad");
    let (mut bridge, _) = Bridge::start(&server, &["--task-launcher", "builtin"]).await;
    for (dir, branch) in [
        (repo.app.to_str().unwrap(), "--force"),
        (repo.app.to_str().unwrap(), "a..b"),
        ("relative", "x"),
    ] {
        let (created, result) = new_task(&mut bridge, Path::new(dir), branch).await;
        assert_eq!(created, None);
        let Message::Result {
            error: Some(error), ..
        } = result
        else {
            panic!("{dir} {branch}: {result:?}")
        };
        assert_eq!(error.code, ErrorCode::BadRequest, "{dir} {branch}");
    }
    assert_eq!(sessions(&server), ["work"]);
}

async fn close_task(
    bridge: &mut Bridge,
    session: &str,
    mode: &str,
) -> Option<muxa_protocol::ErrorBody> {
    let result = bridge
        .request(json!({"type": "close-task", "session": session, "mode": mode}))
        .await;
    let Message::Result { error, .. } = result else {
        panic!("{result:?}")
    };
    error
}

fn commit_in(dir: &Path, file: &str) {
    std::fs::write(dir.join(file), "work\n").unwrap();
    git(dir, &["add", "."]);
    git(
        dir,
        &[
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@t",
            "commit",
            "-q",
            "-m",
            "work",
        ],
    );
}

#[tokio::test]
async fn the_builtin_launcher_closes_only_what_can_be_lost_safely() {
    let server = Server::start();
    let repo = Repo::new("close");
    let (mut bridge, hello) = Bridge::start(
        &server,
        &["--task-launcher", "builtin", "--task-command", "sleep 600"],
    )
    .await;
    let Message::Hello { capabilities, .. } = hello else {
        panic!("{hello:?}")
    };
    assert!(capabilities.contains(&"close-task".to_owned()));
    let worktrees = repo.root.join("repos/.worktrees");

    // A task with work on its branch, watched by this bridge.
    new_task(&mut bridge, &repo.app, "feat/x").await;
    let x = server.tmux(&["display-message", "-p", "-t", "=feat-x:", "#{session_id}"]);
    assert_eq!(
        server.tmux(&["show-options", "-v", "-t", "=feat-x:", "@agent_worktree"]),
        worktrees.join("app-feat-x").display().to_string()
    );
    let wt = worktrees.join("app-feat-x");
    let pane = server.pane("=feat-x:");
    let watch = bridge.request(json!({"type": "watch", "pane": pane})).await;
    assert!(
        matches!(watch, Message::Result { error: None, .. }),
        "{watch:?}"
    );

    std::fs::write(wt.join("README.md"), "changed\n").unwrap();
    let error = close_task(&mut bridge, &x, "force")
        .await
        .expect("closed with changes");
    assert_eq!(error.code, ErrorCode::TaskFailed);
    assert!(
        error.message.contains("uncommitted changes"),
        "{}",
        error.message
    );

    git(&wt, &["checkout", "--", "README.md"]);
    commit_in(&wt, "work.txt");
    let error = close_task(&mut bridge, &x, "close")
        .await
        .expect("closed unmerged");
    assert!(error.message.contains("not merged"), "{}", error.message);
    assert!(sessions(&server).contains(&"feat-x".to_owned()));

    // Keep: the session goes, the worktree stays, and this bridge keeps working.
    assert_eq!(close_task(&mut bridge, &x, "keep").await, None);
    assert!(!sessions(&server).contains(&"feat-x".to_owned()));
    assert!(wt.join("work.txt").is_file());
    let ping = bridge.request(json!({"type": "ping"})).await;
    assert!(
        matches!(ping, Message::Result { error: None, .. }),
        "{ping:?}"
    );

    // A branch with nothing new is merged: its worktree goes, the branch stays.
    new_task(&mut bridge, &repo.app, "feat/y").await;
    let y = server.tmux(&["display-message", "-p", "-t", "=feat-y:", "#{session_id}"]);
    assert_eq!(close_task(&mut bridge, &y, "close").await, None);
    assert!(!worktrees.join("app-feat-y").exists());
    git(
        &repo.app,
        &["rev-parse", "--verify", "-q", "refs/heads/feat/y"],
    );

    // Force removes an unmerged worktree, still keeping the branch.
    new_task(&mut bridge, &repo.app, "feat/z").await;
    let z = server.tmux(&["display-message", "-p", "-t", "=feat-z:", "#{session_id}"]);
    commit_in(&worktrees.join("app-feat-z"), "z.txt");
    assert_eq!(close_task(&mut bridge, &z, "force").await, None);
    assert!(!worktrees.join("app-feat-z").exists());
    git(
        &repo.app,
        &["rev-parse", "--verify", "-q", "refs/heads/feat/z"],
    );
}

#[tokio::test]
async fn sessions_without_a_worktree_are_not_tasks() {
    let server = Server::start();
    let (mut bridge, _) = Bridge::start(&server, &["--task-launcher", "builtin"]).await;
    let work = server.tmux(&["display-message", "-p", "-t", "=work:", "#{session_id}"]);
    for session in [work.as_str(), "$999", "work"] {
        let error = close_task(&mut bridge, session, "close")
            .await
            .expect("closed");
        assert_eq!(error.code, ErrorCode::BadRequest, "{session}");
    }
    assert_eq!(sessions(&server), ["work"]);
}

#[tokio::test]
async fn tmux_agents_closes_with_its_own_script() {
    let server = Server::start();
    let repo = Repo::new("agents-close");
    let scripts = repo.root.join("scripts");
    let new_sh = fake_new_sh(&scripts, false);
    std::fs::write(
        scripts.join("close.sh"),
        format!(
            "#!/bin/sh\nprintf '%s\\n' \"$@\" > '{}'\n",
            scripts.join("close-args").display()
        ),
    )
    .unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(
        scripts.join("close.sh"),
        std::fs::Permissions::from_mode(0o755),
    )
    .unwrap();
    let flag = format!("tmux-agents={}", new_sh.display());
    let (mut bridge, _) = Bridge::start(&server, &["--task-launcher", &flag]).await;
    new_task(&mut bridge, &repo.app, "feat/x").await;
    let id = server.tmux(&[
        "display-message",
        "-p",
        "-t",
        "=fake-task:",
        "#{session_id}",
    ]);
    server.tmux(&["set-option", "-t", &id, "@agent_worktree", "/somewhere"]);

    assert_eq!(close_task(&mut bridge, &id, "keep").await, None);
    let args = std::fs::read_to_string(scripts.join("close-args")).unwrap();
    assert_eq!(args.lines().collect::<Vec<_>>(), ["--keep", "fake-task"]);
}
