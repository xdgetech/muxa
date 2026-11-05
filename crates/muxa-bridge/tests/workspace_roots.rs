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

fn session_ids(server: &Server) -> Vec<String> {
    server
        .tmux(&["list-sessions", "-F", "#{session_id}"])
        .lines()
        .map(str::to_owned)
        .collect()
}

/// The sessions of the clients that are not control clients: the desktop's.
fn desktop_sessions(server: &Server) -> Vec<String> {
    server
        .tmux(&["list-clients", "-F", "#{client_flags} #{session_id}"])
        .lines()
        .filter(|l| !l.contains("control-mode"))
        .filter_map(|l| l.rsplit(' ').next().map(str::to_owned))
        .collect()
}

async fn close(bridge: &mut Bridge, session: &str, mode: &str) -> Message {
    let req = json!({"type": "close-task", "session": session, "mode": mode});
    send(bridge, req).await.1
}

/// ADR 0039: `close-task` closes an ask session, its device session first,
/// and moves the desktop off it. The main checkout keeps every file. A user
/// session with no worktree and no `@muxa_ask` is refused.
#[tokio::test]
async fn close_task_closes_an_ask_session_and_refuses_a_user_session() {
    let server = Server::start();
    let mut entity = Entity::new("close-ask");
    let mut bridge = entity.bridge(&server, false).await;
    let app = entity.root.join("app");
    std::fs::write(app.join("notes.txt"), "uncommitted").unwrap();
    let head = git(&app, &["rev-parse", "HEAD"]);
    let work = server.tmux(&["display-message", "-p", "-t", "=work:", "#{session_id}"]);

    let (started, result) = ask(&mut bridge, json!({"repo": "app"})).await;
    ok(&result);
    let (_, session, name) = started.expect("session-started");
    let Message::Result {
        sessions: Some(listed),
        ..
    } = bridge.request(json!({"type": "list"})).await
    else {
        panic!("no sessions")
    };
    let listed = listed.iter().find(|s| s.name == name).expect("listed");
    assert_eq!(listed.ask.as_deref(), Some("lisk"));

    // The desktop shows the ask session, and the device watches it: a device
    // session holds its window.
    let desktop = common::desktop::Desktop::attach(&server, &session, 120, 40);
    common::wait_until(|| desktop_sessions(&server) == [session.clone()]).await;
    let pane = server.pane(&format!("{session}:"));
    bridge
        .request(json!({"type": "viewport", "cols": 100, "rows": 30}))
        .await;
    bridge.request(json!({"type": "watch", "pane": pane})).await;
    common::wait_until(|| session_ids(&server).len() == 3).await;

    // A user session without a worktree is not the bridge's to close.
    let plain = server.tmux(&[
        "new-session",
        "-d",
        "-P",
        "-F",
        "#{session_id}",
        "-s",
        "plain",
        "sh",
    ]);
    let before = server.layout();
    for mode in ["keep", "close", "force"] {
        let result = close(&mut bridge, &plain, mode).await;
        assert_eq!(error_code(&result), ErrorCode::BadRequest, "{mode}");
    }
    assert_eq!(server.layout(), before);

    // `force` removes nothing: an ask session has no worktree.
    ok(&close(&mut bridge, &session, "force").await);
    let mut left = session_ids(&server);
    left.sort();
    let mut expected = vec![work.clone(), plain.clone()];
    expected.sort();
    assert_eq!(left, expected, "no ask or device session is left");
    assert_eq!(desktop_sessions(&server).len(), 1);
    assert_ne!(
        desktop_sessions(&server),
        [session.clone()],
        "the desktop moved"
    );
    assert_eq!(
        std::fs::read_to_string(app.join("notes.txt")).unwrap(),
        "uncommitted"
    );
    assert_eq!(git(&app, &["rev-parse", "HEAD"]), head);
    assert!(!entity.root.join(".worktrees").exists());

    // Closed already: the id names no session.
    let result = close(&mut bridge, &session, "keep").await;
    assert_eq!(error_code(&result), ErrorCode::BadRequest);
    desktop.close();
}

/// An open review of a Task in `lisk`, stored as a bridge stores it.
fn store_review(entity: &Entity) -> &'static str {
    let dir = entity.dir.join("state/items");
    std::fs::create_dir_all(&dir).unwrap();
    let item = json!({
        "id": "i-0000a5c0", "key": "review:1", "kind": "review", "state": "open",
        "question": "Ready for review: Release notes", "workspace": "lisk",
        "title": "Release notes", "createdAt": 1, "summary": "**v0.7** is ready.",
        "task": "t-0000a5", "attachments": {"links": [{"url": "https://example.com/pr/7"}]}
    });
    let file = json!({"version": 1, "task": "t-0000a5", "items": [item]});
    std::fs::write(dir.join("t-0000a5.json"), file.to_string()).unwrap();
    "i-0000a5c0"
}

/// An agent that reports idle through its pane's tags, then echoes what it gets.
fn idle_agent(entity: &Entity) -> PathBuf {
    let script = entity.dir.join("idle-agent.sh");
    std::fs::write(
        &script,
        "#!/bin/sh\n\
         tmux set-option -p -t \"$TMUX_PANE\" @agent_state_at \"$(date +%s)\"\n\
         tmux set-option -p -t \"$TMUX_PANE\" @agent_state idle\n\
         exec cat\n",
    )
    .unwrap();
    std::fs::set_permissions(&script, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
    let toml = entity.dir.join("workspaces.toml");
    let text = std::fs::read_to_string(&toml).unwrap().replacen(
        "\n\n[[workspace]]\nname = \"other\"",
        &format!(
            "\n\n[workspace.agent]\ncommand = \"{}\"\n\n[[workspace]]\nname = \"other\"",
            script.display()
        ),
        1,
    );
    std::fs::write(&toml, text).unwrap();
    script
}

#[tokio::test]
async fn ask_agent_types_the_item_into_the_workspaces_one_ask_session() {
    let server = Server::start();
    let mut entity = Entity::new("ask-agent");
    idle_agent(&entity);
    let item = store_review(&entity);
    let before = server.layout();
    let mut bridge = entity.bridge(&server, false).await;

    let ask_agent = |text: &str| json!({"type": "update-inbox", "entry": item, "action": "ask-agent", "text": text});
    let (messages, result) = send(&mut bridge, ask_agent("Put this in Notion.")).await;
    ok(&result);
    let started = messages.iter().find_map(|m| match m {
        Message::SessionStarted { session, name, .. } => Some((session.clone(), name.clone())),
        _ => None,
    });
    let (session, name) = started.expect("session-started");
    assert_eq!(name, "ask-lisk");
    assert!(
        messages
            .iter()
            .any(|m| matches!(m, Message::InboxSaved { item: Some(i), .. }
            if i.id == item && i.state == muxa_protocol::ItemState::Open)),
        "the item stays open: {messages:?}"
    );
    assert_eq!(option(&server, &session, "@muxa_ask"), "lisk");
    assert_eq!(option(&server, &session, "@muxa_ask_root"), "1");
    let pane = server.pane(&format!("{session}:"));
    assert_eq!(
        server.tmux(&["display-message", "-p", "-t", &pane, "#{pane_current_path}"]),
        entity.root.display().to_string()
    );
    common::wait_until(|| {
        let shown = server.capture(&pane).join("\n");
        shown.contains("Ready for review: Release notes")
            && shown.contains("**v0.7** is ready.")
            && shown.contains("- https://example.com/pr/7")
            && shown.contains("My request: Put this in Notion.")
    })
    .await;

    // A second request goes to the same session, also while the agent works.
    server.tmux(&["set-option", "-p", "-t", &pane, "@agent_state", "running"]);
    let (messages, result) = send(&mut bridge, ask_agent("And draft a reply.")).await;
    ok(&result);
    assert!(
        messages
            .iter()
            .any(|m| matches!(m, Message::SessionStarted { session: s, .. } if *s == session))
    );
    common::wait_until(|| {
        server
            .capture(&pane)
            .join("\n")
            .contains("My request: And draft a reply.")
    })
    .await;

    // A menu would take the text as its answer: refused, nothing typed.
    server.tmux(&["set-option", "-p", "-t", &pane, "@agent_state", "waiting"]);
    let (_, result) = send(&mut bridge, ask_agent("Never typed.")).await;
    assert_eq!(error_code(&result), ErrorCode::Conflict);
    assert!(!server.capture(&pane).join("\n").contains("Never typed."));

    let (_, result) = send(&mut bridge, ask_agent("")).await;
    assert_eq!(error_code(&result), ErrorCode::BadRequest);

    // Only the new session was added; no pane that was there changed.
    let after = server.layout();
    for line in before.lines() {
        assert!(after.lines().any(|l| l == line), "{line} changed:\n{after}");
    }
}
