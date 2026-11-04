//! Workspaces as Entities (ADR 0031) end to end: repos under the root, a
//! branch from the title, and ask sessions. Each test has its own tmux
//! server; no pane that existed before changes.
#![cfg(feature = "tmux-integration")]

mod common;

use std::path::{Path, PathBuf};
use std::time::Duration;

use common::daemon::Daemon;
use common::repo::git;
use common::{Bridge, Server};
use muxa_protocol::{ErrorCode, Message, TaskRecord, WORK_ASK, WorkspaceInfo, work_level};
use serde_json::{Value, json};

/// Echoes what is typed; enough of an agent here.
const AGENT: &str = "cat";

/// An Entity folder `lisk` with `app` and `group/plan`, and an empty
/// workspace `other` beside it.
struct Entity {
    dir: PathBuf,
    root: PathBuf,
    daemon: Option<Daemon>,
}

impl Entity {
    fn new(name: &str) -> Self {
        let dir =
            std::env::temp_dir().join(format!("muxa-ws-roots-it-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("lisk")).unwrap();
        std::fs::create_dir_all(dir.join("other")).unwrap();
        let dir = dir.canonicalize().unwrap();
        let root = dir.join("lisk");
        init(&root.join("app"));
        init(&root.join("group/plan"));
        std::fs::write(
            dir.join("workspaces.toml"),
            format!(
                "[[workspace]]\nname = \"lisk\"\nroot = \"{}\"\n\n\
                 [[workspace]]\nname = \"other\"\nroot = \"{}\"\n",
                root.display(),
                dir.join("other").display()
            ),
        )
        .unwrap();
        Self {
            dir,
            root,
            daemon: None,
        }
    }

    /// A bridge; with `daemon`, the record requests go to a host daemon.
    async fn bridge(&mut self, server: &Server, daemon: bool) -> Bridge {
        let mut args: Vec<String> = [
            "--state-dir",
            &self.dir.join("state").display().to_string(),
            "--workspaces",
            &self.dir.join("workspaces.toml").display().to_string(),
            "--task-launcher",
            "builtin",
            "--task-command",
            AGENT,
        ]
        .map(String::from)
        .into();
        let daemon_dir = if daemon {
            let flags: Vec<&str> = args.iter().map(String::as_str).collect();
            let started = Daemon::start(server, &flags).await;
            let dir = started.dir.clone();
            self.daemon = Some(started);
            dir
        } else {
            self.dir.join("no-daemon")
        };
        args.extend(["--daemon-dir".to_owned(), daemon_dir.display().to_string()]);
        let args: Vec<&str> = args.iter().map(String::as_str).collect();
        let (bridge, hello) = Bridge::start(server, &args).await;
        let Message::Hello { capabilities, .. } = hello else {
            panic!("{hello:?}")
        };
        assert!(work_level(&capabilities) >= WORK_ASK);
        bridge
    }
}

impl Drop for Entity {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn init(repo: &Path) {
    std::fs::create_dir_all(repo).unwrap();
    git(repo, &["init", "-q", "-b", "main"]);
    std::fs::write(repo.join("README.md"), "# repo\n").unwrap();
    git(repo, &["add", "."]);
    git(
        repo,
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
}

/// The messages before the result, and the result.
async fn send(bridge: &mut Bridge, req: Value) -> (Vec<Message>, Message) {
    let result = bridge.request_within(req, Duration::from_secs(60)).await;
    let mut before = Vec::new();
    while let Some(msg) = bridge.event(Duration::from_millis(200)).await {
        before.push(msg);
    }
    (before, result)
}

async fn create(bridge: &mut Bridge, fields: Value) -> (Option<TaskRecord>, Message) {
    let mut req = json!({"type": "create-task", "workspace": "lisk", "title": "Fix the login page!", "goal": "Make login work."});
    for (k, v) in fields.as_object().unwrap() {
        req[k] = v.clone();
    }
    let (before, result) = send(bridge, req).await;
    let saved = before.into_iter().find_map(|m| match m {
        Message::TaskSaved { record, .. } => Some(record),
        _ => None,
    });
    (saved, result)
}

/// The `session-started` of a `start-session`, and the result.
async fn ask(bridge: &mut Bridge, fields: Value) -> (Option<(u64, String, String)>, Message) {
    let mut req = json!({"type": "start-session", "workspace": "lisk"});
    for (k, v) in fields.as_object().unwrap() {
        req[k] = v.clone();
    }
    let (before, result) = send(bridge, req).await;
    let started = before.into_iter().find_map(|m| match m {
        Message::SessionStarted {
            start,
            session,
            name,
        } => Some((start, session, name)),
        _ => None,
    });
    (started, result)
}

fn ok(result: &Message) {
    assert!(
        matches!(result, Message::Result { error: None, .. }),
        "{result:?}"
    );
}

fn error_code(result: &Message) -> ErrorCode {
    match result {
        Message::Result { error: Some(e), .. } => e.code,
        _ => panic!("{result:?}"),
    }
}

async fn workspaces(bridge: &mut Bridge) -> Vec<WorkspaceInfo> {
    let (before, result) = send(bridge, json!({"type": "list-tasks"})).await;
    ok(&result);
    before
        .into_iter()
        .find_map(|m| match m {
            Message::TaskList { workspaces, .. } => Some(workspaces),
            _ => None,
        })
        .expect("task-list")
}

/// The panes of `before`, as they are now.
fn same_panes(server: &Server, before: &str) -> String {
    server
        .layout()
        .lines()
        .filter(|l| {
            before
                .lines()
                .any(|b| b.split(' ').nth(3) == l.split(' ').nth(3))
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn option(server: &Server, session: &str, name: &str) -> String {
    server.tmux(&["show-options", "-qv", "-t", session, name])
}

#[tokio::test]
async fn the_task_list_has_the_root_and_the_repos_under_it() {
    let server = Server::start();
    let mut entity = Entity::new("list");
    let mut bridge = entity.bridge(&server, false).await;
    let all = workspaces(&mut bridge).await;
    let lisk = all.iter().find(|w| w.name == "lisk").unwrap();
    assert_eq!(lisk.root.as_deref(), entity.root.to_str());
    let repos: Vec<String> = ["app", "group/plan"]
        .iter()
        .map(|r| entity.root.join(r).display().to_string())
        .collect();
    assert_eq!(lisk.repos, repos);
    let other = all.iter().find(|w| w.name == "other").unwrap();
    assert!(other.repos.is_empty());
}

#[tokio::test]
async fn a_repo_task_without_a_branch_gets_one_from_its_title() {
    for daemon in [false, true] {
        let server = Server::start();
        let mut entity = Entity::new(&format!("branch-{daemon}"));
        let before = server.layout();
        let mut bridge = entity.bridge(&server, daemon).await;
        let plan = entity.root.join("group/plan");

        let (saved, result) = create(&mut bridge, json!({"repo": "group/plan"})).await;
        ok(&result);
        let r = saved.expect("task-saved");
        assert_eq!(r.branch.as_deref(), Some("fix-the-login-page"));
        assert_eq!(r.repo.as_deref(), plan.to_str());
        let worktree = entity.root.join("group/.worktrees/plan-fix-the-login-page");
        assert_eq!(r.dir.as_deref(), worktree.to_str());
        assert_eq!(
            git(&worktree, &["branch", "--show-current"]).trim(),
            "fix-the-login-page"
        );
        // The base is the default branch; the main checkout stays on it.
        assert_eq!(git(&plan, &["branch", "--show-current"]).trim(), "main");
        assert_eq!(
            git(&worktree, &["rev-parse", "HEAD"]),
            git(&plan, &["rev-parse", "main"])
        );

        let (again, result) = create(&mut bridge, json!({"repo": "group/plan"})).await;
        ok(&result);
        assert_eq!(
            again.unwrap().branch.as_deref(),
            Some("fix-the-login-page-2")
        );

        for wrong in [
            json!({"repo": "../other"}),
            json!({"repo": "nowhere"}),
            json!({"repo": "app", "workspace": "other"}),
            // Under lisk's root, so it belongs to lisk.
            json!({"repo": entity.root.join("app"), "workspace": "other"}),
            json!({"repo": "app", "title": " "}),
        ] {
            let (saved, result) = create(&mut bridge, wrong.clone()).await;
            assert_eq!(saved, None, "{wrong}");
            assert_eq!(error_code(&result), ErrorCode::BadRequest, "{wrong}");
        }
        assert_eq!(same_panes(&server, &before), before);
    }
}

#[tokio::test]
async fn an_ask_session_runs_in_the_main_checkout_as_a_manual_session() {
    let server = Server::start();
    let mut entity = Entity::new("ask");
    // A second window, so a layout change anywhere would show.
    server.tmux(&["split-window", "-d", "-t", "work:", "sh"]);
    server.tmux(&["new-window", "-d", "-t", "work:", "sh"]);
    let before = server.layout();
    let current = server.tmux(&[
        "display-message",
        "-p",
        "-t",
        "work:",
        "#{window_id} #{pane_id}",
    ]);
    let mut bridge = entity.bridge(&server, false).await;
    let app = entity.root.join("app");

    let (started, result) = ask(&mut bridge, json!({"repo": "app"})).await;
    ok(&result);
    let (start, session, name) = started.expect("session-started");
    assert_eq!(start, bridge.last_id());
    assert!(session.starts_with('$'), "{session}");
    assert_eq!(name, "ask-app-1");

    let pane = server.pane(&format!("{session}:"));
    assert_eq!(
        server.tmux(&[
            "display-message",
            "-p",
            "-t",
            &pane,
            "#{@muxa_role} #{pane_current_path}"
        ]),
        format!("agent {}", app.display())
    );
    assert_eq!(option(&server, &session, "@muxa_task"), "");
    assert_eq!(option(&server, &session, "@muxa_device"), "");
    assert_eq!(option(&server, &session, "@muxa_ask"), "lisk");
    // No worktree and no branch: the main checkout as it was.
    assert!(!entity.root.join(".worktrees").exists());
    assert_eq!(git(&app, &["branch", "--show-current"]).trim(), "main");
    assert_eq!(git(&app, &["branch", "--list"]).lines().count(), 1);

    // Listed as a manual session: no taskId, no worktree.
    let Message::Result {
        sessions: Some(sessions),
        ..
    } = bridge.request(json!({"type": "list"})).await
    else {
        panic!("no sessions")
    };
    let listed = sessions.iter().find(|s| s.name == name).expect("listed");
    assert_eq!(listed.task_id, None);
    assert_eq!(listed.worktree, None);
    assert_eq!(listed.repo_dir.as_deref(), app.to_str());

    let (second, result) = ask(&mut bridge, json!({"repo": entity.root.join("app")})).await;
    ok(&result);
    assert_eq!(second.unwrap().2, "ask-app-2");
    let (plan, result) = ask(&mut bridge, json!({"repo": "group/plan"})).await;
    ok(&result);
    assert_eq!(plan.unwrap().2, "ask-plan-1");

    for wrong in [
        json!({"workspace": "nowhere", "repo": "app"}),
        json!({"repo": "../other"}),
        json!({"repo": "nowhere"}),
        json!({"repo": "app", "workspace": "other"}),
        json!({"repo": app, "workspace": "other"}),
        json!({"repo": ""}),
    ] {
        let (started, result) = ask(&mut bridge, wrong.clone()).await;
        assert_eq!(started, None, "{wrong}");
        assert_eq!(error_code(&result), ErrorCode::BadRequest, "{wrong}");
    }

    assert_eq!(same_panes(&server, &before), before);
    assert_eq!(
        server.tmux(&[
            "display-message",
            "-p",
            "-t",
            "work:",
            "#{window_id} #{pane_id}"
        ]),
        current,
        "the user's session keeps its current window and pane"
    );
}
