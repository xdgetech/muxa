//! Reading Tasks end to end (ADR 0034): a Task in its own folder that reads
//! a repo, Branch to a worktree, Park and Resume, the changed-checkout item,
//! and the daemon's `park_after`. Throwaway tmux servers and temp dirs only.
#![cfg(feature = "tmux-integration")]

mod common;

use std::path::{Path, PathBuf};
use std::time::Duration;

use common::daemon::Daemon;
use common::repo::{Repo, git};
use common::{Bridge, Server};
use muxa_protocol::{ErrorCode, Message, RunReason, TaskRecord, TaskStatus, WORK_READ, work_level};
use serde_json::{Value, json};

/// Echoes what is typed; no adapter reports it.
const AGENT: &str = "cat";
const SLOW: Duration = Duration::from_secs(90);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    Bridge,
    Daemon,
}

/// A state dir, a workspace root and its `workspaces.toml`; in daemon mode,
/// a daemon serves the record requests.
struct Host {
    dir: PathBuf,
    root: PathBuf,
    daemon: Option<Daemon>,
}

impl Host {
    fn new(name: &str, extra: &str) -> Self {
        let dir =
            std::env::temp_dir().join(format!("muxa-reading-it-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("work")).unwrap();
        let dir = dir.canonicalize().unwrap();
        let root = dir.join("work");
        std::fs::write(
            dir.join("workspaces.toml"),
            format!(
                "[[workspace]]\nname = \"work\"\nroot = \"{}\"\n{extra}",
                root.display()
            ),
        )
        .unwrap();
        Self {
            dir,
            root,
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

    async fn bridge(&mut self, server: &Server, mode: Mode, command: &str) -> Bridge {
        let mut args = self.args();
        args.extend(["--task-launcher", "builtin", "--task-command", command].map(str::to_owned));
        let daemon_dir = match mode {
            Mode::Daemon => {
                if self.daemon.is_none() {
                    let flags: Vec<&str> = args.iter().map(String::as_str).collect();
                    self.daemon = Some(Daemon::start(server, &flags).await);
                }
                self.daemon.as_ref().map(|d| d.dir.clone()).unwrap()
            }
            Mode::Bridge => self.dir.join("no-daemon"),
        };
        args.extend(["--daemon-dir".to_owned(), daemon_dir.display().to_string()]);
        let args: Vec<&str> = args.iter().map(String::as_str).collect();
        let (bridge, hello) = Bridge::start(server, &args).await;
        let Message::Hello { capabilities, .. } = hello else {
            panic!("{hello:?}")
        };
        // Levels add up: the bridge's level 5 (ADR 0039) has reading Tasks.
        assert!(work_level(&capabilities) >= WORK_READ);
        bridge
    }
}

impl Drop for Host {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// The `task-saved` before the result, and the result.
async fn send(bridge: &mut Bridge, req: Value) -> (Option<TaskRecord>, Message) {
    let result = bridge.request_within(req, SLOW).await;
    let id = bridge.last_id();
    let mut saved = None;
    while let Some(msg) = bridge.event(Duration::from_millis(200)).await {
        if let Message::TaskSaved { task, record } = msg {
            if task == id {
                saved = Some(record);
            }
        }
    }
    (saved, result)
}

async fn create_reading(bridge: &mut Bridge, repo: &Path) -> TaskRecord {
    let (saved, result) = send(
        bridge,
        json!({"type": "create-task", "workspace": "work", "title": "Review the auth design",
               "goal": "Read the code and write a review.", "repo": repo, "read": true}),
    )
    .await;
    ok(&result);
    saved.expect("task-saved")
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

fn read(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// `#{pane_current_path}` of the session's first pane, canonical.
fn pane_path(server: &Server, session: &str) -> PathBuf {
    let path = server.tmux(&[
        "display-message",
        "-p",
        "-t",
        &format!("={session}:"),
        "#{pane_current_path}",
    ]);
    PathBuf::from(path).canonicalize().unwrap()
}

fn sessions(server: &Server) -> Vec<String> {
    server
        .tmux(&["list-sessions", "-F", "#{session_name}"])
        .lines()
        .map(str::to_owned)
        .collect()
}

async fn wait_for(what: &str, mut cond: impl FnMut() -> bool) {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(20);
    while !cond() {
        assert!(tokio::time::Instant::now() < deadline, "no {what}");
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
}

#[tokio::test]
async fn reading_tasks_branch_park_and_resume_in_the_bridge() {
    reading_task_lifecycle(Mode::Bridge).await;
}

#[tokio::test]
async fn reading_tasks_branch_park_and_resume_in_the_daemon() {
    reading_task_lifecycle(Mode::Daemon).await;
}

async fn reading_task_lifecycle(mode: Mode) {
    let server = Server::start();
    let mut host = Host::new(&format!("life-{mode:?}"), "");
    let repo = Repo::new(&format!("reading-{mode:?}"));
    let before = server.tmux(&[
        "list-panes",
        "-a",
        "-F",
        "#{pane_id} #{pane_width}x#{pane_height}",
    ]);
    let mut bridge = host.bridge(&server, mode, AGENT).await;

    // 1. A reading Task: its own folder, the repo to read, no worktree.
    let r = create_reading(&mut bridge, &repo.app).await;
    assert!(r.read);
    assert_eq!(r.branch, None);
    assert_eq!(r.repo.as_deref(), Some(repo.app.to_str().unwrap()));
    let folder = PathBuf::from(r.dir.clone().unwrap());
    assert_eq!(folder.parent(), Some(host.root.join("tasks").as_path()));
    let session = r.session.clone().unwrap();
    assert_eq!(pane_path(&server, &session), folder);
    let brief = read(&folder.join(".muxa/TASK.md"));
    assert!(brief.contains("## Reading the repo"), "{brief}");
    assert!(brief.contains(&format!("git -C {}", repo.app.display())));
    assert!(
        !folder.join(".claude/settings.local.json").exists(),
        "only for Claude Code"
    );
    assert_eq!(git(&repo.app, &["worktree", "list"]).lines().count(), 1);
    assert_eq!(git(&repo.app, &["status", "--porcelain"]), "");

    // The agent's notes and output in the folder.
    let notes = "<!-- muxa:notes - muxa never changes anything below this line. -->\n\n- Read the auth code.\n";
    let at = brief.find("<!-- muxa:notes").unwrap();
    std::fs::write(
        folder.join(".muxa/TASK.md"),
        format!("{}{notes}", &brief[..at]),
    )
    .unwrap();
    std::fs::write(folder.join("review.md"), "# Review\n").unwrap();
    let agent_pane = server.pane(&format!("={session}:"));

    // A reading Task has no loop, and only a reading Task branches.
    let (saved, result) = send(
        &mut bridge,
        json!({"type": "create-task", "workspace": "work", "title": "x", "goal": "g",
               "repo": repo.app, "read": true, "branch": "b"}),
    )
    .await;
    assert_eq!((saved, error_code(&result)), (None, ErrorCode::BadRequest));

    // 2. Branch: a worktree with the same rules, TASK.md copied, the agent restarted there.
    let (saved, result) = send(&mut bridge, json!({"type": "branch-task", "task": r.id})).await;
    ok(&result);
    let b = saved.expect("task-saved");
    assert!(!b.read);
    assert_eq!(b.branch.as_deref(), Some("review-the-auth-design"));
    let worktree = repo
        .root
        .join("repos/.worktrees/app-review-the-auth-design");
    assert_eq!(b.dir.as_deref(), Some(worktree.to_str().unwrap()));
    assert_eq!(b.runs.len(), 2);
    assert_eq!(b.runs[1].reason, RunReason::Restart);
    let moved = read(&worktree.join(".muxa/TASK.md"));
    assert!(moved.ends_with(notes), "the notes came along: {moved}");
    assert!(!moved.contains("## Reading the repo"));
    assert!(moved.contains(&format!("this Task worked in {}", folder.display())));
    let handoff = read(&worktree.join(".muxa/HANDOFF.md"));
    assert!(handoff.contains("## Task folder"), "{handoff}");
    assert!(handoff.contains("- review.md"), "{handoff}");
    assert!(folder.join("review.md").exists(), "the folder stays");
    assert_eq!(
        git(&worktree, &["status", "--porcelain"]),
        "",
        ".muxa/ is excluded"
    );
    // Same pane, now in the worktree; the session carries the worktree tags.
    assert_eq!(server.pane(&format!("={session}:")), agent_pane);
    wait_for("the pane in the worktree", || {
        pane_path(&server, &session) == worktree
    })
    .await;
    let tag = server.tmux(&[
        "show-options",
        "-qv",
        "-t",
        &format!("={session}:"),
        "@agent_worktree",
    ]);
    assert_eq!(tag, worktree.display().to_string());
    let (_, result) = send(&mut bridge, json!({"type": "branch-task", "task": r.id})).await;
    assert_eq!(
        error_code(&result),
        ErrorCode::BadRequest,
        "branched already"
    );

    // 3. Park: HANDOFF.md, the session gone, the Task and worktree kept.
    let (saved, result) = send(&mut bridge, json!({"type": "park-task", "task": r.id})).await;
    ok(&result);
    let p = saved.expect("task-saved");
    assert_eq!(p.status, TaskStatus::Open);
    assert_eq!(p.session, None);
    assert!(p.runs.last().unwrap().ended_at.is_some());
    assert!(!sessions(&server).contains(&session));
    assert!(read(&worktree.join(".muxa/HANDOFF.md")).contains("when it parked"));
    assert!(worktree.join(".muxa/TASK.md").exists());
    let (_, result) = send(&mut bridge, json!({"type": "park-task", "task": r.id})).await;
    assert_eq!(error_code(&result), ErrorCode::BadRequest, "parked already");

    // 4. Resume is restart-agent: a new session in the worktree.
    let (saved, result) = send(&mut bridge, json!({"type": "restart-agent", "task": r.id})).await;
    ok(&result);
    let resumed = saved.expect("task-saved");
    assert_eq!(resumed.session.as_deref(), Some("review-the-auth-design"));
    assert_eq!(resumed.runs.last().unwrap().reason, RunReason::Restart);
    assert_eq!(resumed.runs.last().unwrap().ended_at, None);
    let tag = server.tmux(&[
        "show-options",
        "-qv",
        "-t",
        "=review-the-auth-design:",
        "@muxa_task",
    ]);
    assert_eq!(tag, r.id);
    assert_eq!(pane_path(&server, "review-the-auth-design"), worktree);

    // No pane that existed before changed size (invariant 2).
    let now = server.tmux(&[
        "list-panes",
        "-a",
        "-F",
        "#{pane_id} #{pane_width}x#{pane_height}",
    ]);
    for line in before.lines() {
        assert!(now.lines().any(|l| l == line), "{line} changed: {now}");
    }
}

#[tokio::test]
async fn claude_code_gets_settings_that_deny_edits_in_the_checkout() {
    let server = Server::start();
    let mut host = Host::new("claude", "");
    let repo = Repo::new("reading-claude");
    // A stand-in named claude, so the bridge sees Claude Code's command.
    let bin = host.dir.join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    let claude = bin.join("claude");
    std::fs::write(&claude, "#!/bin/sh\nexec cat\n").unwrap();
    std::process::Command::new("chmod")
        .args(["+x", claude.to_str().unwrap()])
        .status()
        .unwrap();
    let mut bridge = host
        .bridge(&server, Mode::Bridge, claude.to_str().unwrap())
        .await;
    let r = create_reading(&mut bridge, &repo.app).await;
    let folder = PathBuf::from(r.dir.unwrap());
    let settings: Value =
        serde_json::from_str(&read(&folder.join(".claude/settings.local.json"))).unwrap();
    let main = repo.app.display().to_string();
    assert_eq!(
        settings,
        json!({"permissions": {
            "additionalDirectories": [main],
            "deny": [format!("Edit(/{main}/**)")],
        }})
    );
}

#[tokio::test]
async fn a_turn_that_changes_the_main_checkout_asks_branch_or_keep() {
    let server = Server::start();
    let mut host = Host::new("changed", "");
    let repo = Repo::new("reading-changed");
    let mut bridge = host.bridge(&server, Mode::Bridge, AGENT).await;
    let r = create_reading(&mut bridge, &repo.app).await;
    let pane = server.pane(&format!("={}:", r.session.unwrap()));
    let start = r.runs[0].started_at;
    let report = |state: &str, at: u64| {
        server.tmux(&["set-option", "-p", "-t", &pane, "@agents_role", "ai"]);
        server.tmux(&[
            "set-option",
            "-p",
            "-t",
            &pane,
            "@agent_state_at",
            &at.to_string(),
        ]);
        server.tmux(&["set-option", "-p", "-t", &pane, "@agent_state", state]);
    };
    let items_file = host.dir.join(format!("state/items/{}.json", r.id));
    let items = || -> Value {
        std::fs::read_to_string(&items_file)
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or(Value::Null)
    };

    // The turn starts: the bridge notes the checkout's status once.
    report("running", start + 1);
    wait_for("the turn's start status", || {
        items().get("readBaseline").is_some()
    })
    .await;
    std::fs::write(repo.app.join("README.md"), "# app, changed\n").unwrap();
    std::fs::write(repo.app.join("new.txt"), "x\n").unwrap();
    report("idle", start + 2);
    wait_for("the proposal", || {
        items()["items"]
            .as_array()
            .is_some_and(|i| i.iter().any(|x| x["kind"] == "proposal"))
    })
    .await;
    let item = items()["items"][0].clone();
    assert_eq!(item["question"], "The agent changed the main checkout");
    assert_eq!(item["answers"], json!(["branch", "keep", "ask-agent", "later"]));
    assert_eq!(item["attachments"]["changes"]["files"], 2);
    let summary = item["summary"].as_str().unwrap();
    assert!(
        summary.contains(" M README.md") && summary.contains("?? new.txt"),
        "{summary}"
    );
    // Never reverted.
    assert_eq!(read(&repo.app.join("README.md")), "# app, changed\n");

    // Keep answers it and changes nothing.
    let result = bridge
        .request(json!({"type": "update-inbox", "entry": item["id"], "action": "keep"}))
        .await;
    ok(&result);
    assert_eq!(items()["items"][0]["state"], "answered");
    assert!(repo.app.join("new.txt").exists());

    // A turn that changes nothing new makes a review, not a proposal.
    report("running", start + 3);
    wait_for("the next start status", || {
        items()["readBaseline"]["since"] == json!(start + 3)
    })
    .await;
    report("idle", start + 4);
    wait_for("the review", || {
        items()["items"]
            .as_array()
            .is_some_and(|i| i.iter().any(|x| x["kind"] == "review"))
    })
    .await;
}

#[tokio::test]
async fn the_daemon_parks_a_task_idle_past_park_after() {
    let server = Server::start();
    let host = Host::new("park-after", "park_after = \"1m\"\n");
    let folder = host.root.join("tasks/idle-t-00c0de");
    std::fs::create_dir_all(&folder).unwrap();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let record = |id: &str, dir: &Path| {
        json!({
            "version": 1, "id": id, "rev": 1, "workspace": "work",
            "title": "Idle", "goal": "g", "status": "open",
            "createdAt": now - 600, "updatedAt": now - 600,
            "dir": dir.display().to_string(),
            "runs": [{"id": 1, "startedAt": now - 600, "reason": "start"}],
        })
    };
    let state = host.dir.join("state");
    std::fs::create_dir_all(&state).unwrap();
    let busy = host.root.join("tasks/busy-t-00beef");
    std::fs::create_dir_all(&busy).unwrap();
    for (id, dir) in [("t-00c0de", &folder), ("t-00beef", &busy)] {
        std::fs::write(
            state.join(format!("{id}.json")),
            record(id, dir).to_string(),
        )
        .unwrap();
        let made = server.tmux(&[
            "new-session",
            "-d",
            "-P",
            "-F",
            "#{pane_id}",
            "-s",
            id,
            "-c",
            &dir.display().to_string(),
            "sleep 600",
        ]);
        server.tmux(&["set-option", "-t", &format!("={id}:"), "@muxa_task", id]);
        server.tmux(&["set-option", "-p", "-t", &made, "@agents_role", "ai"]);
        let (state, at) = if id == "t-00c0de" {
            ("idle", now - 120)
        } else {
            ("idle", now - 10)
        };
        server.tmux(&[
            "set-option",
            "-p",
            "-t",
            &made,
            "@agent_state_at",
            &at.to_string(),
        ]);
        server.tmux(&["set-option", "-p", "-t", &made, "@agent_state", state]);
    }
    // The user answered the idle Task's review: nothing is open for it.
    std::fs::create_dir_all(state.join("items")).unwrap();
    let answered = json!({"version": 1, "task": "t-00c0de", "items": [{
        "id": "i-0000c0de", "kind": "review", "state": "answered",
        "question": "Ready for review: Idle", "workspace": "work", "title": "Idle",
        "createdAt": now - 120, "answeredAt": now - 100, "answer": "keep-open",
        "task": "t-00c0de", "key": format!("review:{}", now - 120),
    }]});
    std::fs::write(state.join("items/t-00c0de.json"), answered.to_string()).unwrap();
    let flags = host.args();
    let flags: Vec<&str> = flags.iter().map(String::as_str).collect();
    let daemon = Daemon::start(&server, &flags).await;
    wait_for("the idle task parked", || {
        !sessions(&server).contains(&"t-00c0de".to_owned())
    })
    .await;
    assert!(read(&folder.join(".muxa/HANDOFF.md")).contains("when it parked"));
    let saved: Value = serde_json::from_str(&read(&state.join("t-00c0de.json"))).unwrap();
    assert_eq!(saved["status"], "open");
    assert!(saved.get("session").is_none());
    assert!(
        daemon.log().contains("idle task parked"),
        "{}",
        daemon.log()
    );
    // Idle for 10 s only: it keeps its session.
    tokio::time::sleep(Duration::from_secs(3)).await;
    assert!(sessions(&server).contains(&"t-00beef".to_owned()));
}
