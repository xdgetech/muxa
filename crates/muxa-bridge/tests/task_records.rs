//! Task records end to end (ADR 0015): `create-task` with and without a repo,
//! the `@muxa_task` link and its fallback, the start prompt, and `rev`
//! conflicts. Records go to a temp state directory; no existing pane changes.
#![cfg(feature = "tmux-integration")]

mod common;

use std::path::{Path, PathBuf};
use std::time::Duration;

use common::repo::{Repo, fake_new_sh, git};
use common::{Bridge, Server};
use muxa_protocol::{ErrorCode, Message, TaskRecord, TaskStatus};
use serde_json::{Value, json};

const PROMPT: &str = "Read .muxa/TASK.md and start.";
/// Echoes what is typed, so the screen shows whether the prompt arrived.
const AGENT: &str = "cat";

/// A state directory, a workspace root and the `workspaces.toml` naming it.
struct Host {
    dir: PathBuf,
    root: PathBuf,
}

impl Host {
    fn new(name: &str) -> Self {
        let dir =
            std::env::temp_dir().join(format!("muxa-records-it-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("work")).unwrap();
        let dir = dir.canonicalize().unwrap();
        let root = dir.join("work");
        std::fs::write(
            dir.join("workspaces.toml"),
            format!(
                "[[workspace]]\nname = \"work\"\nroot = \"{}\"\n",
                root.display()
            ),
        )
        .unwrap();
        Self { dir, root }
    }

    fn args(&self) -> Vec<String> {
        vec![
            "--state-dir".into(),
            self.dir.join("state").display().to_string(),
            "--workspaces".into(),
            self.dir.join("workspaces.toml").display().to_string(),
        ]
    }

    async fn bridge(&self, server: &Server, extra: &[&str]) -> Bridge {
        let mut args = self.args();
        args.extend(extra.iter().map(|s| s.to_string()));
        let args: Vec<&str> = args.iter().map(String::as_str).collect();
        let (bridge, hello) = Bridge::start(server, &args).await;
        let Message::Hello { capabilities, .. } = hello else {
            panic!("{hello:?}")
        };
        assert!(capabilities.contains(&"task-records".to_owned()));
        bridge
    }
}

impl Drop for Host {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// The `task-saved` sent before the result, and the result.
async fn send(bridge: &mut Bridge, req: Value) -> (Option<TaskRecord>, Message) {
    let result = bridge.request(req).await;
    let id = bridge.last_id();
    let mut saved = None;
    while let Some(msg) = bridge.event(Duration::from_millis(200)).await {
        if let Message::TaskSaved { task, record } = msg {
            assert_eq!(task, id);
            saved = Some(record);
        }
    }
    (saved, result)
}

async fn create(bridge: &mut Bridge, fields: Value) -> (Option<TaskRecord>, Message) {
    let mut req = json!({"type": "create-task", "workspace": "work", "title": "Fix the login page!", "goal": "Make login work.\nSecret plan."});
    for (k, v) in fields.as_object().unwrap() {
        req[k] = v.clone();
    }
    send(bridge, req).await
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

async fn list_tasks(bridge: &mut Bridge) -> Vec<TaskRecord> {
    let result = bridge.request(json!({"type": "list-tasks"})).await;
    ok(&result);
    let id = bridge.last_id();
    while let Some(msg) = bridge.event(Duration::from_millis(200)).await {
        if let Message::TaskList {
            tasks,
            workspaces,
            entries,
            ..
        } = msg
        {
            assert_eq!(tasks, id);
            assert_eq!(workspaces.len(), 1);
            assert_eq!(workspaces[0].name, "work");
            return entries;
        }
    }
    panic!("no task-list")
}

/// Session name -> taskId, from `list`.
async fn task_ids(bridge: &mut Bridge) -> Vec<(String, Option<String>)> {
    let Message::Result {
        sessions: Some(sessions),
        ..
    } = bridge.request(json!({"type": "list"})).await
    else {
        panic!("no sessions")
    };
    sessions.into_iter().map(|s| (s.name, s.task_id)).collect()
}

fn option(server: &Server, session: &str, name: &str) -> String {
    server.tmux(&["show-options", "-qv", "-t", &format!("={session}:"), name])
}

/// The existing panes, as `Server::layout` shows them, but only those of `before`.
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

fn prompts(server: &Server, pane: &str) -> usize {
    server
        .capture(pane)
        .iter()
        .filter(|l| l.as_str() == PROMPT)
        .count()
}

#[tokio::test]
async fn a_task_without_a_repo_gets_its_own_directory_and_session() {
    let server = Server::start();
    let host = Host::new("dir");
    let before = server.layout();
    let mut bridge = host
        .bridge(
            &server,
            &["--task-launcher", "builtin", "--task-command", AGENT],
        )
        .await;
    assert!(list_tasks(&mut bridge).await.is_empty());

    let (saved, result) = create(&mut bridge, json!({"project": "auth"})).await;
    ok(&result);
    let r = saved.expect("task-saved");
    let dir = host.root.join(format!("tasks/fix-the-login-page-{}", r.id));
    assert_eq!(r.dir.as_deref(), Some(dir.to_str().unwrap()));
    assert_eq!(r.status, TaskStatus::Open);
    assert_eq!(r.note, None);
    let name = r.session.clone().expect("session");
    assert_eq!(name, format!("fix-the-login-page-{}", r.id));

    // The session runs there, tagged, with the agent pane tagged too.
    assert_eq!(option(&server, &name, "@muxa_task"), r.id);
    let pane = server.pane(&format!("={name}:"));
    assert_eq!(
        server.tmux(&[
            "display-message",
            "-p",
            "-t",
            &pane,
            "#{@muxa_role} #{pane_current_path}"
        ]),
        format!("agent {}", dir.display())
    );
    let task_md = std::fs::read_to_string(dir.join(".muxa/TASK.md")).unwrap();
    assert!(task_md.starts_with("# Fix the login page!\n"), "{task_md}");
    assert!(task_md.contains("Make login work.\nSecret plan."));
    assert!(task_md.contains("- Project: auth"));
    assert!(
        task_ids(&mut bridge)
            .await
            .contains(&(name.clone(), Some(r.id.clone())))
    );
    assert_eq!(list_tasks(&mut bridge).await, std::slice::from_ref(&r));

    // The prompt waits for the agent to be idle, then is typed once.
    tokio::time::sleep(Duration::from_millis(1500)).await;
    assert_eq!(prompts(&server, &pane), 0);
    server.tmux(&["set-option", "-p", "-t", &pane, "@agent_state", "idle"]);
    common::wait_until(|| prompts(&server, &pane) > 0).await;
    tokio::time::sleep(Duration::from_millis(2500)).await;
    // `cat` shows the typed line, then prints it back.
    assert_eq!(prompts(&server, &pane), 2);

    assert_eq!(same_panes(&server, &before), before);
}

#[tokio::test]
async fn a_repo_task_runs_in_its_worktree_with_task_md_excluded() {
    let server = Server::start();
    let host = Host::new("repo");
    let repo = Repo::new("records-repo");
    let before = server.layout();
    let mut bridge = host
        .bridge(
            &server,
            &["--task-launcher", "builtin", "--task-command", AGENT],
        )
        .await;

    let (saved, result) = create(
        &mut bridge,
        json!({"repo": repo.app, "branch": "feat/login"}),
    )
    .await;
    ok(&result);
    let r = saved.expect("task-saved");
    let worktree = repo.root.join("repos/.worktrees/app-feat-login");
    assert_eq!(r.dir.as_deref(), Some(worktree.to_str().unwrap()));
    assert_eq!(r.repo.as_deref(), Some(repo.app.to_str().unwrap()));
    assert_eq!(r.branch.as_deref(), Some("feat/login"));
    assert_eq!(r.session.as_deref(), Some("feat-login"));
    assert_eq!(option(&server, "feat-login", "@muxa_task"), r.id);
    assert!(
        std::fs::read_to_string(worktree.join(".muxa/TASK.md"))
            .unwrap()
            .contains("- Branch: feat/login")
    );
    // TASK.md is not a change in the worktree.
    assert_eq!(git(&worktree, &["status", "--porcelain"]), "");
    let exclude = std::fs::read_to_string(repo.app.join(".git/info/exclude")).unwrap();
    assert_eq!(exclude.lines().filter(|l| *l == ".muxa/").count(), 1);
    assert_eq!(same_panes(&server, &before), before);
}

#[tokio::test]
async fn a_branch_that_runs_an_open_task_is_refused() {
    let server = Server::start();
    let host = Host::new("linked");
    let repo = Repo::new("records-linked");
    let mut bridge = host
        .bridge(
            &server,
            &["--task-launcher", "builtin", "--task-command", AGENT],
        )
        .await;
    let branch = json!({"repo": repo.app, "branch": "feat/x"});
    let (first, result) = create(&mut bridge, branch.clone()).await;
    ok(&result);
    let first = first.unwrap();

    let (saved, result) = create(&mut bridge, branch.clone()).await;
    assert_eq!(saved, None);
    assert_eq!(error_code(&result), ErrorCode::BadRequest);

    // Wrong requests keep no record either.
    for wrong in [
        json!({"workspace": "nowhere"}),
        json!({"repo": repo.app}),
        json!({"branch": "feat/y"}),
        json!({"repo": repo.app, "branch": "a..b"}),
        json!({"repo": "/no/such/repo", "branch": "feat/y"}),
        json!({"title": " "}),
    ] {
        let (saved, result) = create(&mut bridge, wrong.clone()).await;
        assert_eq!(saved, None, "{wrong}");
        assert_eq!(error_code(&result), ErrorCode::BadRequest, "{wrong}");
    }
    assert_eq!(list_tasks(&mut bridge).await.len(), 1);

    // Once that task is done, the branch can take a new one.
    let (_, result) = send(
        &mut bridge,
        json!({"type": "update-task", "task": first.id, "rev": first.rev, "status": "done"}),
    )
    .await;
    ok(&result);
    let (second, result) = create(&mut bridge, branch).await;
    ok(&result);
    assert_eq!(option(&server, "feat-x", "@muxa_task"), second.unwrap().id);
}

#[tokio::test]
async fn a_stale_rev_is_a_conflict_and_changes_nothing() {
    let server = Server::start();
    let host = Host::new("conflict");
    let mut bridge = host
        .bridge(
            &server,
            &["--task-launcher", "builtin", "--task-command", AGENT],
        )
        .await;
    let (r, result) = create(&mut bridge, json!({})).await;
    ok(&result);
    let r = r.unwrap();
    let update = |rev: u64, status: &str| json!({"type": "update-task", "task": r.id, "rev": rev, "status": status});

    let (saved, result) = send(&mut bridge, update(r.rev, "review")).await;
    ok(&result);
    let saved = saved.unwrap();
    assert_eq!((saved.rev, saved.status), (r.rev + 1, TaskStatus::Review));

    let (none, result) = send(&mut bridge, update(r.rev, "dropped")).await;
    assert_eq!(none, None);
    assert_eq!(error_code(&result), ErrorCode::Conflict);
    let listed = list_tasks(&mut bridge).await;
    assert_eq!(listed[0].status, TaskStatus::Review);

    let (_, result) = send(
        &mut bridge,
        json!({"type": "update-task", "task": "t-000000", "rev": 1, "status": "done"}),
    )
    .await;
    assert_eq!(error_code(&result), ErrorCode::BadRequest);
}

#[tokio::test]
async fn without_the_tag_a_session_links_by_its_directory() {
    let server = Server::start();
    let host = Host::new("fallback");
    let mut bridge = host
        .bridge(
            &server,
            &["--task-launcher", "builtin", "--task-command", AGENT],
        )
        .await;
    let (r, result) = create(&mut bridge, json!({})).await;
    ok(&result);
    let r = r.unwrap();
    let name = r.session.clone().unwrap();
    let dir = r.dir.clone().unwrap();
    drop(bridge);

    // A restored server: the session is back in its directory, its options gone.
    server.tmux(&["kill-session", "-t", &format!("={name}")]);
    server.tmux(&["new-session", "-d", "-s", "restored", "-c", &dir, "sh"]);
    let mut bridge = host.bridge(&server, &[]).await;
    let ids = task_ids(&mut bridge).await;
    assert!(
        ids.contains(&("restored".into(), Some(r.id.clone()))),
        "{ids:?}"
    );
    assert!(ids.contains(&("work".into(), None)), "{ids:?}");
    // Listing writes nothing to tmux.
    assert_eq!(option(&server, "restored", "@muxa_task"), "");
}

#[tokio::test]
async fn tmux_agents_tasks_are_found_by_their_worktree() {
    let server = Server::start();
    let host = Host::new("agents");
    let repo = Repo::new("records-agents");
    let script = fake_new_sh(&repo.root.join("scripts"), false);
    let flag = format!("tmux-agents={}", script.display());
    let mut bridge = host.bridge(&server, &["--task-launcher", &flag]).await;

    let (r, result) = create(&mut bridge, json!({"repo": repo.app, "branch": "feat/x"})).await;
    ok(&result);
    let r = r.unwrap();
    let worktree = repo.root.join("repos/.worktrees/app-feat-x");
    assert_eq!(r.session.as_deref(), Some("fake-task"));
    assert_eq!(r.dir.as_deref(), Some(worktree.to_str().unwrap()));
    assert_eq!(option(&server, "fake-task", "@muxa_task"), r.id);
    assert!(worktree.join(".muxa/TASK.md").is_file());

    // Its agent pane gets the prompt once tmux-agents' hook says idle. The
    // terminal echoes it; `sleep` reads nothing.
    let pane = server.pane("=fake-task:");
    server.tmux(&["set-option", "-p", "-t", &pane, "@agent_state", "idle"]);
    common::wait_until(|| prompts(&server, &pane) == 1).await;
}

#[tokio::test]
async fn a_client_that_leaves_right_away_still_gets_its_start_prompt() {
    let server = Server::start();
    let host = Host::new("leaves");
    let mut bridge = host
        .bridge(
            &server,
            &["--task-launcher", "builtin", "--task-command", AGENT],
        )
        .await;
    let (r, result) = create(&mut bridge, json!({})).await;
    ok(&result);
    let name = r.unwrap().session.unwrap();
    let pane = server.pane(&format!("={name}:"));

    // Like `muxa-cli task new`: close the input once the result is in.
    let (mut child, _output) = bridge.close_input();
    tokio::time::sleep(Duration::from_millis(1500)).await;
    assert!(
        child.try_wait().unwrap().is_none(),
        "the bridge did not wait"
    );
    server.tmux(&["set-option", "-p", "-t", &pane, "@agent_state", "idle"]);
    common::wait_until(|| prompts(&server, &pane) > 0).await;
    let exited = tokio::time::timeout(Duration::from_secs(5), child.wait()).await;
    assert!(exited.is_ok(), "the bridge did not exit after the prompt");
}

#[tokio::test]
async fn tmux_agents_gets_the_record_when_it_takes_it() {
    let server = Server::start();
    let host = Host::new("features");
    let repo = Repo::new("records-features");
    let scripts = repo.root.join("scripts");
    let script = fake_new_sh(&scripts, false);
    server.tmux(&["set-option", "-g", "@agents_features", "muxa-task"]);
    let flag = format!("tmux-agents={}", script.display());
    let mut bridge = host.bridge(&server, &["--task-launcher", &flag]).await;

    let (r, result) = create(&mut bridge, json!({"repo": repo.app, "branch": "feat/x"})).await;
    ok(&result);
    let r = r.unwrap();
    let args = std::fs::read_to_string(scripts.join("args")).unwrap();
    assert_eq!(
        args.lines().collect::<Vec<_>>(),
        [
            repo.app.to_str().unwrap(),
            "feat/x",
            "--muxa-task",
            &r.id,
            "--no-switch",
            &format!("socket={}", server.socket),
        ]
    );
    assert_eq!(option(&server, "fake-task", "@muxa_task"), r.id);
}

#[tokio::test]
async fn a_failed_launch_keeps_the_record_with_a_note() {
    let server = Server::start();
    let host = Host::new("failed");
    let repo = Repo::new("records-failed");
    let script = fake_new_sh(&repo.root.join("scripts"), true);
    let flag = format!("tmux-agents={}", script.display());
    let mut bridge = host.bridge(&server, &["--task-launcher", &flag]).await;

    let (r, result) = create(&mut bridge, json!({"repo": repo.app, "branch": "feat/x"})).await;
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
    let r = r.expect("the record is kept");
    assert_eq!(r.session, None);
    assert!(r.note.unwrap().contains("boom"));
    assert_eq!(list_tasks(&mut bridge).await.len(), 1);
    assert!(Path::new(&host.dir.join("state")).is_dir());
}
