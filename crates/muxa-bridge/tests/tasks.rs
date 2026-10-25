//! `new-task` end to end: the built-in launcher, and tmux-agents through a fake
//! `new.sh`. Neither may change an existing pane.
#![cfg(feature = "tmux-integration")]

mod common;

use std::path::{Path, PathBuf};
use std::process::Command;

use common::{Bridge, Server};
use muxa_protocol::{ErrorCode, Message};
use serde_json::json;

/// `<tmp>/repos/app` with one commit, removed on drop with its worktrees.
struct Repo {
    root: PathBuf,
    app: PathBuf,
}

impl Repo {
    fn new(name: &str) -> Self {
        let root =
            std::env::temp_dir().join(format!("muxa-tasks-it-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let app = root.join("repos/app");
        std::fs::create_dir_all(&app).unwrap();
        let app = app.canonicalize().unwrap();
        git(&app, &["init", "-q", "-b", "main"]);
        std::fs::write(app.join("README.md"), "# app\n").unwrap();
        git(&app, &["add", "."]);
        git(
            &app,
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
        Self {
            root: root.canonicalize().unwrap(),
            app,
        }
    }
}

impl Drop for Repo {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn git(dir: &Path, args: &[&str]) {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .unwrap();
    assert!(out.status.success(), "git {args:?}");
}

/// `new.sh` stand-in: records its arguments and socket, then makes a session on
/// that socket, or fails like tmux-agents' `die`.
fn fake_new_sh(dir: &Path, fail: bool) -> PathBuf {
    std::fs::create_dir_all(dir).unwrap();
    let script = dir.join("new.sh");
    let log = dir.join("args");
    let body = if fail {
        "echo 'agents: could not create a worktree for feat/x: boom' >&2\nexit 1\n".to_owned()
    } else {
        "tmux -L \"$TMUX_AGENTS_SOCKET\" new-session -d -s fake-task 'sleep 600'\necho fake-task\n"
            .to_owned()
    };
    std::fs::write(
        &script,
        format!(
            "#!/bin/sh\nprintf '%s\\n' \"$@\" \"socket=$TMUX_AGENTS_SOCKET\" > '{}'\n{body}",
            log.display()
        ),
    )
    .unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
    script
}

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
