//! Task records end to end (ADR 0015): `create-task` with and without a repo,
//! the `@muxa_task` link and its fallback, the start prompt, and `rev`
//! conflicts. Records go to a temp state directory; no existing pane changes.
//! Every test runs twice (ADR 0019): with the host daemon serving the
//! records, and with the bridge serving them itself.
#![cfg(feature = "tmux-integration")]

mod common;

use std::path::{Path, PathBuf};
use std::time::Duration;

use common::daemon::Daemon;
use common::repo::{Repo, fake_new_sh, git};
use common::{Bridge, Server};
use muxa_protocol::{AgentState, ErrorCode, Message, RunReason, TaskRecord, TaskStatus};
use serde_json::{Value, json};

const PROMPT: &str = "Read .muxa/TASK.md and start.";
/// Echoes what is typed, so the screen shows whether the prompt arrived.
const AGENT: &str = "cat";

/// Who serves the record requests.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    Bridge,
    Daemon,
}

impl Mode {
    /// A per-mode name, as both modes of a test run at once.
    fn name(self, base: &str) -> String {
        format!("{base}-{self:?}")
    }
}

/// Each test, once per mode.
macro_rules! both {
    ($($name:ident),* $(,)?) => {
        mod bridge {
            $(#[tokio::test]
            async fn $name() {
                super::$name(super::Mode::Bridge).await
            })*
        }
        mod daemon {
            $(#[tokio::test]
            async fn $name() {
                super::$name(super::Mode::Daemon).await
            })*
        }
    };
}

both!(
    another_writer_s_change_is_announced,
    a_task_without_a_repo_gets_its_own_directory_and_session,
    a_fresh_done_gets_the_start_prompt,
    a_repo_task_runs_in_its_worktree_with_task_md_excluded,
    a_branch_that_runs_an_open_task_is_refused,
    a_stale_rev_is_a_conflict_and_changes_nothing,
    without_the_tag_a_session_links_by_its_directory,
    tmux_agents_tasks_are_found_by_their_worktree,
    a_client_that_leaves_right_away_still_gets_its_start_prompt,
    tmux_agents_gets_the_record_when_it_takes_it,
    a_failed_launch_keeps_the_record_with_a_note,
    a_workspace_agent_env_reaches_the_agent_and_its_adapter,
    a_repo_task_gets_its_workspace_env_even_with_tmux_agents,
    a_restart_keeps_the_pane_hands_off_and_records_runs,
    a_running_agent_is_restarted_only_with_force,
    an_agent_typed_into_a_shell_is_typed_again,
);

/// A state directory, a workspace root and the `workspaces.toml` naming it.
/// In daemon mode, the first bridge starts a daemon with the same flags.
struct Host {
    dir: PathBuf,
    root: PathBuf,
    mode: Mode,
    daemon: Option<Daemon>,
}

impl Host {
    fn new(name: &str, mode: Mode) -> Self {
        let name = mode.name(name);
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
        Self {
            dir,
            root,
            mode,
            daemon: None,
        }
    }

    fn args(&self) -> Vec<String> {
        vec![
            "--state-dir".into(),
            self.dir.join("state").display().to_string(),
            "--workspaces".into(),
            self.dir.join("workspaces.toml").display().to_string(),
        ]
    }

    async fn bridge(&mut self, server: &Server, extra: &[&str]) -> Bridge {
        let mut args = self.args();
        args.extend(extra.iter().map(|s| s.to_string()));
        if self.mode == Mode::Daemon {
            if self.daemon.is_none() {
                let flags: Vec<&str> = args.iter().map(String::as_str).collect();
                self.daemon = Some(Daemon::start(server, &flags).await);
            }
            let dir = self.daemon.as_ref().map(|d| d.dir.display().to_string());
            args.extend(["--daemon-dir".to_owned(), dir.unwrap_or_default()]);
        } else {
            // No daemon: a dir where none runs.
            args.extend([
                "--daemon-dir".to_owned(),
                self.dir.join("no-daemon").display().to_string(),
            ]);
        }
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

async fn a_task_without_a_repo_gets_its_own_directory_and_session(mode: Mode) {
    let server = Server::start();
    let mut host = Host::new("dir", mode);
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

fn epoch() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

/// tmux-agents reports a finished turn as `done` while no person watches
/// the window (ADR 0019). Only a `done` newer than the wait counts.
async fn a_fresh_done_gets_the_start_prompt(mode: Mode) {
    let server = Server::start();
    let mut host = Host::new("done", mode);
    let mut bridge = host
        .bridge(
            &server,
            &["--task-launcher", "builtin", "--task-command", AGENT],
        )
        .await;
    let (r, result) = create(&mut bridge, json!({})).await;
    ok(&result);
    let pane = server.pane(&format!("={}:", r.unwrap().session.unwrap()));

    let old = (epoch() - 600).to_string();
    server.tmux(&["set-option", "-p", "-t", &pane, "@agent_state_at", &old]);
    server.tmux(&["set-option", "-p", "-t", &pane, "@agent_state", "done"]);
    tokio::time::sleep(Duration::from_millis(2500)).await;
    assert_eq!(prompts(&server, &pane), 0, "a done from before the wait");

    let fresh = epoch().to_string();
    server.tmux(&["set-option", "-p", "-t", &pane, "@agent_state_at", &fresh]);
    common::wait_until(|| prompts(&server, &pane) > 0).await;
}

async fn a_repo_task_runs_in_its_worktree_with_task_md_excluded(mode: Mode) {
    let server = Server::start();
    let mut host = Host::new("repo", mode);
    let repo = Repo::new(&mode.name("records-repo"));
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

async fn a_branch_that_runs_an_open_task_is_refused(mode: Mode) {
    let server = Server::start();
    let mut host = Host::new("linked", mode);
    let repo = Repo::new(&mode.name("records-linked"));
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

/// The next `tasks-changed` within `wait`.
async fn tasks_changed(bridge: &mut Bridge, wait: Duration) -> bool {
    let deadline = tokio::time::Instant::now() + wait;
    while let Some(left) = deadline.checked_duration_since(tokio::time::Instant::now()) {
        match bridge.event(left).await {
            Some(Message::TasksChanged) => return true,
            Some(_) => {}
            None => return false,
        }
    }
    false
}

/// A record written by another client reaches this one as `tasks-changed`
/// (ADR 0019), whoever wrote it.
async fn another_writer_s_change_is_announced(mode: Mode) {
    let server = Server::start();
    let mut host = Host::new("changed", mode);
    let flags = ["--task-launcher", "builtin", "--task-command", AGENT];
    let mut watching = host.bridge(&server, &flags).await;
    let mut writer = host.bridge(&server, &flags).await;
    // The first look sees the directory as it is; nothing is announced.
    assert!(!tasks_changed(&mut watching, Duration::from_millis(2500)).await);

    let (r, result) = create(&mut writer, json!({})).await;
    ok(&result);
    assert!(tasks_changed(&mut watching, Duration::from_secs(5)).await);
    let r = r.unwrap();
    let (_, result) = send(
        &mut writer,
        json!({"type": "update-task", "task": r.id, "rev": r.rev, "status": "review"}),
    )
    .await;
    ok(&result);
    assert!(tasks_changed(&mut watching, Duration::from_secs(5)).await);
}

async fn a_stale_rev_is_a_conflict_and_changes_nothing(mode: Mode) {
    let server = Server::start();
    let mut host = Host::new("conflict", mode);
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

async fn without_the_tag_a_session_links_by_its_directory(mode: Mode) {
    let server = Server::start();
    let mut host = Host::new("fallback", mode);
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

async fn tmux_agents_tasks_are_found_by_their_worktree(mode: Mode) {
    let server = Server::start();
    let mut host = Host::new("agents", mode);
    let repo = Repo::new(&mode.name("records-agents"));
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

async fn a_client_that_leaves_right_away_still_gets_its_start_prompt(mode: Mode) {
    let server = Server::start();
    let mut host = Host::new("leaves", mode);
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
    match mode {
        Mode::Bridge => assert!(
            child.try_wait().unwrap().is_none(),
            "the bridge did not wait"
        ),
        // The daemon waits; the bridge need not.
        Mode::Daemon => assert!(child.try_wait().unwrap().is_some(), "the bridge waited"),
    }
    server.tmux(&["set-option", "-p", "-t", &pane, "@agent_state", "idle"]);
    common::wait_until(|| prompts(&server, &pane) > 0).await;
    let exited = tokio::time::timeout(Duration::from_secs(5), child.wait()).await;
    assert!(exited.is_ok(), "the bridge did not exit after the prompt");
}

async fn tmux_agents_gets_the_record_when_it_takes_it(mode: Mode) {
    let server = Server::start();
    let mut host = Host::new("features", mode);
    let repo = Repo::new(&mode.name("records-features"));
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

async fn a_failed_launch_keeps_the_record_with_a_note(mode: Mode) {
    let server = Server::start();
    let mut host = Host::new("failed", mode);
    let repo = Repo::new(&mode.name("records-failed"));
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

/// Stands in for Claude Code with a per-workspace `CLAUDE_CONFIG_DIR` (ADR
/// 0016): it writes its environment beside the config dir, then a session
/// file for its pane as Claude Code does, and keeps running.
fn fake_claude(dir: &Path) -> PathBuf {
    std::fs::create_dir_all(dir).unwrap();
    let script = dir.join("fake-claude");
    std::fs::write(
        &script,
        r#"#!/bin/sh
env > "$CLAUDE_CONFIG_DIR.env"
mkdir -p "$CLAUDE_CONFIG_DIR/sessions"
session="$(tmux display-message -p -t "$TMUX_PANE" '#{session_name}')"
printf '{"pid":%s,"sessionId":"0b8c3c0e-1111-4a2b-9c3d-000000000016","cwd":"%s","kind":"interactive","status":"idle","tmux":"%s:@0.%s"}\n' \
  "$$" "$PWD" "$session" "$TMUX_PANE" > "$CLAUDE_CONFIG_DIR/sessions/$$.json"
exec sleep 600
"#,
    )
    .unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
    script
}

/// Token-like: a space, a quote, `$` and `~/` must arrive unchanged.
const TOKEN: &str = r#"tok en"$X~/y"#;

/// `[workspace.agent]` text for a workspace, and its Claude config dir.
fn agent_table(host: &Host) -> (String, PathBuf) {
    let config = host.dir.join("claude-work");
    let table = format!(
        "\n[workspace.agent]\ncommand = \"{}\"\nenv = {{ CLAUDE_CONFIG_DIR = \"{}\", MUXA_IT_TOKEN = '{TOKEN}' }}\n",
        fake_claude(&host.dir.join("bin")).display(),
        config.display(),
    );
    (table, config)
}

/// The agent's environment, once it has started.
async fn agent_env(config: &Path) -> Vec<String> {
    let file = config.with_extension("env");
    common::wait_until(|| file.is_file()).await;
    tokio::time::sleep(Duration::from_millis(200)).await;
    std::fs::read_to_string(&file)
        .unwrap()
        .lines()
        .map(str::to_owned)
        .collect()
}

async fn pane_state(bridge: &mut Bridge, pane: &str) -> Option<AgentState> {
    let Message::Result {
        sessions: Some(sessions),
        ..
    } = bridge.request(json!({"type": "list"})).await
    else {
        panic!("no sessions")
    };
    sessions
        .iter()
        .flat_map(|s| &s.panes)
        .find(|p| p.id == pane)
        .and_then(|p| p.agent.as_ref().map(|a| a.state))
}

async fn a_workspace_agent_env_reaches_the_agent_and_its_adapter(mode: Mode) {
    let server = Server::start();
    let mut host = Host::new("agent-env", mode);
    let (table, config) = agent_table(&host);
    let path = host.dir.join("workspaces.toml");
    let text = std::fs::read_to_string(&path).unwrap();
    std::fs::write(&path, format!("{text}{table}")).unwrap();
    // The default Claude dir is empty: the agent can only be found in the workspace's.
    let empty = host.dir.join("claude-default");
    std::fs::create_dir_all(&empty).unwrap();
    let flag = format!("claude-code={}", empty.display());
    let before = server.layout();
    let mut bridge = host
        .bridge(
            &server,
            &[
                "--task-launcher",
                "builtin",
                "--task-command",
                AGENT,
                "--agent-dir",
                &flag,
            ],
        )
        .await;

    let (saved, result) = create(&mut bridge, json!({})).await;
    ok(&result);
    let r = saved.expect("task-saved");
    let env = agent_env(&config).await;
    assert!(
        env.contains(&format!("CLAUDE_CONFIG_DIR={}", config.display())),
        "{env:?}"
    );
    assert!(env.contains(&format!("MUXA_IT_TOKEN={TOKEN}")), "{env:?}");

    // The adapter finds the session file in the workspace's config dir.
    let name = r.session.expect("session");
    let pane = server.pane(&format!("={name}:"));
    let sessions = config.join("sessions");
    common::wait_until(|| sessions.read_dir().is_ok_and(|mut d| d.next().is_some())).await;
    assert_eq!(pane_state(&mut bridge, &pane).await, Some(AgentState::Idle));
    assert_eq!(same_panes(&server, &before), before);
}

async fn a_repo_task_gets_its_workspace_env_even_with_tmux_agents(mode: Mode) {
    let server = Server::start();
    let mut host = Host::new("agent-env-repo", mode);
    let repo = Repo::new(&mode.name("records-agent-env"));
    let (table, config) = agent_table(&host);
    let other = host.dir.join("other");
    std::fs::create_dir_all(&other).unwrap();
    // `work` lists the repo; `other` lists another directory.
    let path = host.dir.join("workspaces.toml");
    let text = std::fs::read_to_string(&path).unwrap();
    std::fs::write(
        &path,
        format!(
            "{text}repos = [\"{}\"]\n{table}\n[[workspace]]\nname = \"other\"\nroot = \"{}\"\nrepos = [\"{}\"]\n",
            repo.app.display(),
            other.display(),
            repo.root.display(),
        ),
    )
    .unwrap();
    let script = fake_new_sh(&repo.root.join("scripts"), false);
    let flag = format!("tmux-agents={}", script.display());
    let mut bridge = host.bridge(&server, &["--task-launcher", &flag]).await;

    let (r, result) = create(&mut bridge, json!({"repo": repo.app, "branch": "feat/x"})).await;
    ok(&result);
    let r = r.unwrap();
    // The built-in launcher ran: tmux-agents cannot pass the env yet.
    assert_eq!(r.session.as_deref(), Some("feat-x"));
    assert!(!repo.root.join("scripts/args").exists(), "new.sh ran");
    let env = agent_env(&config).await;
    assert!(env.contains(&format!("MUXA_IT_TOKEN={TOKEN}")), "{env:?}");

    // The repo is `work`'s: a Task in `other` would run it with other accounts.
    let (none, result) = create(
        &mut bridge,
        json!({"workspace": "other", "repo": repo.app, "branch": "feat/y"}),
    )
    .await;
    let Message::Result {
        error: Some(error), ..
    } = result
    else {
        panic!("{result:?}")
    };
    assert_eq!(error.code, ErrorCode::BadRequest);
    assert!(
        error.message.contains("belongs to workspace \"work\""),
        "{}",
        error.message
    );
    assert!(none.is_none());
}

const RESTART_PROMPT: &str = "Read .muxa/TASK.md and .muxa/HANDOFF.md, then continue.";
/// The session id `fake_claude` writes.
const FAKE_SESSION: &str = "0b8c3c0e-1111-4a2b-9c3d-000000000016";

async fn restart(bridge: &mut Bridge, task: &str, force: bool) -> (Option<TaskRecord>, Message) {
    send(
        bridge,
        json!({"type": "restart-agent", "task": task, "force": force}),
    )
    .await
}

/// `Server::layout` without the pids, which a restart changes.
fn shape(server: &Server) -> String {
    server
        .layout()
        .lines()
        .map(|l| l.rsplit_once(' ').map_or(l, |(shape, _pid)| shape))
        .collect::<Vec<_>>()
        .join("\n")
}

async fn record_of(bridge: &mut Bridge, id: &str) -> TaskRecord {
    list_tasks(bridge)
        .await
        .into_iter()
        .find(|r| r.id == id)
        .expect("the record")
}

async fn a_restart_keeps_the_pane_hands_off_and_records_runs(mode: Mode) {
    let server = Server::start();
    let mut host = Host::new("restart", mode);
    let (table, config) = agent_table(&host);
    let path = host.dir.join("workspaces.toml");
    let text = std::fs::read_to_string(&path).unwrap();
    std::fs::write(&path, format!("{text}{table}")).unwrap();
    let empty = host.dir.join("claude-default");
    std::fs::create_dir_all(&empty).unwrap();
    let flag = format!("claude-code={}", empty.display());
    let mut bridge = host
        .bridge(
            &server,
            &[
                "--task-launcher",
                "builtin",
                "--task-command",
                AGENT,
                "--agent-dir",
                &flag,
            ],
        )
        .await;

    let (saved, result) = create(&mut bridge, json!({})).await;
    ok(&result);
    let r = saved.expect("task-saved");
    assert_eq!(r.runs.len(), 1);
    assert_eq!(r.runs[0].reason, RunReason::Start);
    let pane = server.pane(&format!("={}:", r.session.clone().unwrap()));
    agent_env(&config).await;
    // The prompt path fills in the run from the adapter.
    common::wait_until(|| prompts(&server, &pane) > 0).await;
    let mut listed = record_of(&mut bridge, &r.id).await;
    for _ in 0..20 {
        if listed.runs[0].agent_session.is_some() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
        listed = record_of(&mut bridge, &r.id).await;
    }
    assert_eq!(listed.runs[0].agent.as_deref(), Some("claude-code"));
    assert_eq!(listed.runs[0].agent_session.as_deref(), Some(FAKE_SESSION));

    // The agent's notes, and a transcript the adapter can point to.
    let dir = PathBuf::from(r.dir.clone().unwrap());
    let brief = dir.join(".muxa/TASK.md");
    let text = std::fs::read_to_string(&brief).unwrap();
    let notes_at = text.find("<!-- muxa:notes").expect("marker");
    let notes = format!("{}\n- Fixed the form.\n", &text[notes_at..]);
    std::fs::write(&brief, format!("{}{notes}", &text[..notes_at])).unwrap();
    let transcript = config.join(format!("projects/x/{FAKE_SESSION}.jsonl"));
    std::fs::create_dir_all(transcript.parent().unwrap()).unwrap();
    std::fs::write(&transcript, "{}\n").unwrap();
    std::fs::remove_file(config.with_extension("env")).unwrap();
    let before = shape(&server);
    let pid = server.tmux(&["display-message", "-p", "-t", &pane, "#{pane_pid}"]);

    let (saved, result) = restart(&mut bridge, &r.id, false).await;
    ok(&result);
    let saved = saved.expect("task-saved");
    assert_eq!(saved.runs.len(), 2);
    let (first, second) = (&saved.runs[0], &saved.runs[1]);
    assert!(first.ended_at.is_some());
    assert_eq!(first.agent_session.as_deref(), Some(FAKE_SESSION));
    assert_eq!(
        first.transcript.as_deref(),
        Some(transcript.to_str().unwrap())
    );
    assert_eq!((second.id, second.reason), (2, RunReason::Restart));
    assert_eq!(second.started_at, first.ended_at.unwrap());

    // Same pane, same size, same layout; a new process with the workspace env.
    let env = agent_env(&config).await;
    assert!(env.contains(&format!("MUXA_IT_TOKEN={TOKEN}")), "{env:?}");
    assert_eq!(shape(&server), before);
    assert_ne!(
        server.tmux(&["display-message", "-p", "-t", &pane, "#{pane_pid}"]),
        pid
    );

    // TASK.md keeps the notes; HANDOFF.md has the facts.
    let text = std::fs::read_to_string(&brief).unwrap();
    assert!(text.ends_with(&notes), "{text}");
    let handoff = std::fs::read_to_string(dir.join(".muxa/HANDOFF.md")).unwrap();
    assert!(handoff.contains("- Run 1 (start)"), "{handoff}");
    assert!(handoff.contains(transcript.to_str().unwrap()), "{handoff}");
    assert!(handoff.contains("## Files changed since"), "{handoff}");

    // The new agent reports idle, then gets the prompt once.
    let restarted = || {
        server
            .capture(&pane)
            .iter()
            .filter(|l| l.as_str() == RESTART_PROMPT)
            .count()
    };
    common::wait_until(|| restarted() == 1).await;
}

async fn a_running_agent_is_restarted_only_with_force(mode: Mode) {
    let server = Server::start();
    let mut host = Host::new("restart-running", mode);
    let mut bridge = host
        .bridge(
            &server,
            &["--task-launcher", "builtin", "--task-command", AGENT],
        )
        .await;
    let (r, result) = create(&mut bridge, json!({})).await;
    ok(&result);
    let r = r.unwrap();
    let pane = server.pane(&format!("={}:", r.session.clone().unwrap()));
    server.tmux(&["set-option", "-p", "-t", &pane, "@agent_state", "running"]);

    let (none, result) = restart(&mut bridge, &r.id, false).await;
    assert!(none.is_none());
    let Message::Result {
        error: Some(error), ..
    } = result
    else {
        panic!("{result:?}")
    };
    assert_eq!(error.code, ErrorCode::BadRequest);
    assert!(error.message.contains("running"), "{}", error.message);
    assert_eq!(record_of(&mut bridge, &r.id).await.runs.len(), 1);
    let dir = PathBuf::from(r.dir.clone().unwrap());
    assert!(!dir.join(".muxa/HANDOFF.md").exists());

    let before = shape(&server);
    let (saved, result) = restart(&mut bridge, &r.id, true).await;
    ok(&result);
    assert_eq!(saved.unwrap().runs.len(), 2);
    assert_eq!(shape(&server), before);

    let (_, result) = restart(&mut bridge, "t-000000", false).await;
    assert_eq!(error_code(&result), ErrorCode::BadRequest);
}

async fn an_agent_typed_into_a_shell_is_typed_again(mode: Mode) {
    let server = Server::start();
    let mut host = Host::new("restart-typed", mode);
    let mut bridge = host
        .bridge(
            &server,
            &["--task-launcher", "builtin", "--task-command", AGENT],
        )
        .await;
    let (r, result) = create(&mut bridge, json!({})).await;
    ok(&result);
    let r = r.unwrap();
    let dir = r.dir.clone().unwrap();
    // As tmux-agents does it: a shell pane, the agent typed into it.
    server.tmux(&["kill-session", "-t", &format!("={}", r.session.unwrap())]);
    server.tmux(&["new-session", "-d", "-s", "typed", "-c", &dir]);
    let pane = server.pane("=typed:");
    server.tmux(&["set-option", "-p", "-t", &pane, "@muxa_role", "agent"]);
    let start = server.tmux(&[
        "display-message",
        "-p",
        "-t",
        &pane,
        "#{pane_start_command}",
    ]);
    assert_eq!(start.trim(), "");
    let before = shape(&server);

    let (saved, result) = restart(&mut bridge, &r.id, false).await;
    ok(&result);
    assert_eq!(saved.unwrap().runs.len(), 2);
    let command = || {
        server
            .tmux(&[
                "display-message",
                "-p",
                "-t",
                &pane,
                "#{pane_current_command}",
            ])
            .trim()
            .to_owned()
    };
    common::wait_until(|| command() == AGENT).await;
    assert_eq!(shape(&server), before);
}

/// The record file as the store wrote it, with fields the wire leaves out.
fn record_file(host: &Host, id: &str) -> Value {
    let text = std::fs::read_to_string(host.dir.join(format!("state/{id}.json"))).unwrap();
    serde_json::from_str(&text).unwrap()
}

/// A crash does not lose a prompt wait (ADR 0019): the next daemon resumes
/// one younger than 30 s, and types the prompt once.
#[tokio::test]
async fn a_daemon_that_crashed_during_a_prompt_wait_resumes_it() {
    let server = Server::start();
    let mut host = Host::new("resume", Mode::Daemon);
    let flags = ["--task-launcher", "builtin", "--task-command", AGENT];
    let mut bridge = host.bridge(&server, &flags).await;
    let (r, result) = create(&mut bridge, json!({})).await;
    ok(&result);
    let r = r.unwrap();
    let pane = server.pane(&format!("={}:", r.session.clone().unwrap()));
    let wait = record_file(&host, &r.id)["pendingPrompt"].clone();
    assert_eq!(wait["kind"], "start", "{wait}");
    assert_eq!(wait["run"], 1, "{wait}");

    let mut daemon = host.daemon.take().expect("the daemon");
    daemon.crash().await;
    let mut args = host.args();
    args.extend(flags.iter().map(|s| s.to_string()));
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    daemon.start_again(&server, &args).await;
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert_eq!(
        prompts(&server, &pane),
        0,
        "typed before the agent was idle"
    );

    server.tmux(&["set-option", "-p", "-t", &pane, "@agent_state", "idle"]);
    common::wait_until(|| prompts(&server, &pane) > 0).await;
    common::wait_until(|| record_file(&host, &r.id).get("pendingPrompt").is_none()).await;
    tokio::time::sleep(Duration::from_millis(2500)).await;
    // `cat` shows the typed line, then prints it back: typed once.
    assert_eq!(prompts(&server, &pane), 2);
    assert!(daemon.log().contains("resuming a prompt wait"));

    // A wait older than 30 s ends with the note, and types nothing.
    let path = host.dir.join(format!("state/{}.json", r.id));
    let mut file = record_file(&host, &r.id);
    file["pendingPrompt"] = json!({"kind": "start", "since": epoch() - 100, "run": 1});
    std::fs::write(&path, file.to_string()).unwrap();
    daemon.crash().await;
    daemon.start_again(&server, &args).await;
    common::wait_until(|| record_file(&host, &r.id).get("pendingPrompt").is_none()).await;
    let note = record_file(&host, &r.id)["note"].clone();
    assert!(
        note.as_str().is_some_and(|n| n.contains("was not sent")),
        "{note}"
    );
    assert_eq!(prompts(&server, &pane), 2);
    host.daemon = Some(daemon);
}
