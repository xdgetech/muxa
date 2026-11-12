//! The assistant proposes Tasks (ADR 0060) end to end: a real daemon on a
//! throwaway tmux server runs the lead, the fake agent of
//! `data/assistant-agent.py`, whose Result proposes a Task. Its `result`
//! item offers Start Task; approve starts one Task linked to the Work item,
//! with Claude Code's plan flag, also when sent twice. The secretary's
//! `propose-task` makes a `new-task` item that starts a Task the same way.
//! Titles and goals never reach the log.
#![cfg(feature = "tmux-integration")]

mod common;

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use common::Server;
use common::daemon::{Conn, Daemon};
use muxa_protocol::{
    AssistantRequest, Delivery, InboxAction, InboxItem, ItemKind, ItemState, Message, TaskRecord,
    WorkItem,
};
use serde_json::{Value, json};

const AGENT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/data/assistant-agent.py");
const LEAD: &str = "muxa-lead";
const MEMBER: &str = "muxa-review";
/// In the proposed goal; never in the daemon's log.
const GOAL_MARK: &str = "goal-mark-7c21";
const TITLE_MARK: &str = "title-mark-90ab";
const RUN_WAIT: Duration = Duration::from_secs(120);
const PLAN_FLAG: &str = "--permission-mode plan";

struct Host {
    server: Server,
    base: PathBuf,
}

impl Drop for Host {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.base);
    }
}

fn git(dir: &Path, args: &[&str]) {
    let out = Command::new("git")
        .args(["-c", "user.name=t", "-c", "user.email=t@t"])
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap();
    assert!(out.status.success(), "git {args:?}: {out:?}");
}

impl Host {
    /// Workspace `muxa` with the repo `app` and a staff of two, its lead first.
    fn new(name: &str) -> Host {
        let base =
            std::env::temp_dir().join(format!("muxa-proposals-it-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        for d in ["muxa/app", "state", "steer", "bin", "claude"] {
            std::fs::create_dir_all(base.join(d)).unwrap();
        }
        let base = base.canonicalize().unwrap();
        let app = base.join("muxa/app");
        git(&app, &["init", "-q", "-b", "main"]);
        std::fs::write(app.join("README.md"), "# app\n").unwrap();
        git(&app, &["add", "."]);
        git(&app, &["commit", "-q", "-m", "init"]);
        let claude = base.join("bin/claude");
        std::fs::write(&claude, format!("#!/bin/sh\nexec python3 {AGENT} \"$@\"\n")).unwrap();
        std::fs::set_permissions(&claude, std::fs::Permissions::from_mode(0o755)).unwrap();
        let worker = |name: &str, job: &str| {
            format!(
                "[[worker]]\nname = \"{name}\"\nagent = \"claude-code\"\nargs = [\"--agent\", \"{name}\"]\n\
                 description = \"{job}\"\n"
            )
        };
        std::fs::write(
            base.join("workspaces.toml"),
            format!(
                "[assistant]\nenabled = true\n\
                 [[workspace]]\nname = \"muxa\"\nroot = \"{root}\"\n\
                 [workspace.agent]\nenv = {{ PATH = \"{bin}:{path}\", ASSISTANT_STEER = \"{steer}\", \
                 CLAUDE_CONFIG_DIR = \"{claude}\" }}\n\
                 [workspace.assistant]\ndelegate = true\nmax_runs_per_day = 20\nmax_open_runs = 3\n\
                 staff = [\"{LEAD}\", \"{MEMBER}\"]\n\
                 {}{}",
                worker(LEAD, "Lead: research, read-only."),
                worker(MEMBER, "Review: read-only."),
                root = base.join("muxa").display(),
                bin = base.join("bin").display(),
                steer = base.join("steer").display(),
                path = std::env::var("PATH").unwrap_or_default(),
                claude = base.join("claude").display(),
            ),
        )
        .unwrap();
        Host {
            server: Server::start(),
            base,
        }
    }

    async fn daemon(&self) -> Daemon {
        let flags = [
            "--state-dir".to_owned(),
            self.base.join("state").display().to_string(),
            "--workspaces".into(),
            self.base.join("workspaces.toml").display().to_string(),
            "--task-launcher".into(),
            "builtin".into(),
            "--task-command".into(),
            format!(
                "{} {}",
                self.base.join("bin/claude").display(),
                self.base.join("steer").display()
            ),
        ];
        let flags: Vec<&str> = flags.iter().map(String::as_str).collect();
        Daemon::start(&self.server, &flags).await
    }

    fn steer(&self, name: &str, value: &Value) {
        std::fs::write(self.base.join("steer").join(name), value.to_string()).unwrap();
    }

    /// The arguments each started agent got, one line each.
    fn argv(&self) -> Vec<String> {
        std::fs::read_to_string(self.base.join("steer/argv.log"))
            .unwrap_or_default()
            .lines()
            .map(str::to_owned)
            .collect()
    }

    fn record(&self, id: &str) -> Value {
        let text = std::fs::read_to_string(self.base.join(format!("state/{id}.json"))).unwrap();
        serde_json::from_str(&text).unwrap()
    }

    /// Every Task whose goal ends with a proposal's marker.
    fn proposed_tasks(&self) -> Vec<Value> {
        std::fs::read_dir(self.base.join("state"))
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().starts_with("t-"))
            .filter_map(|e| std::fs::read_to_string(e.path()).ok())
            .filter_map(|t| serde_json::from_str::<Value>(&t).ok())
            .filter(|r| {
                r["goal"]
                    .as_str()
                    .and_then(|g| g.lines().next_back())
                    .is_some_and(|l| l.starts_with("muxa-task-proposal: "))
            })
            .collect()
    }

    /// The agent of `dir` started with the plan flag.
    async fn until_planning(&self, started_before: usize) -> String {
        let deadline = Instant::now() + RUN_WAIT;
        loop {
            let argv = self.argv();
            if argv.len() > started_before {
                return argv[started_before].clone();
            }
            assert!(Instant::now() < deadline, "no agent started: {argv:?}");
            tokio::time::sleep(Duration::from_millis(250)).await;
        }
    }
}

fn proposing() -> Value {
    json!({"version": 1, "request": "@", "requestVersion": 0,
           "summary": "The crash comes from an empty summary.",
           "task": {"repo": "app", "title": format!("Fix the crash {TITLE_MARK}"),
                    "goal": format!("Guard the empty summary. {GOAL_MARK}")}})
}

async fn work_item(conn: &mut Conn, client: &str) -> WorkItem {
    let saved = conn
        .request(json!({"type": "capture", "clientId": client,
                        "text": "Fix the Inbox crash", "workspace": "muxa"}))
        .await;
    let Some(Message::CaptureSaved { entry, .. }) = saved.first() else {
        panic!("{saved:?}")
    };
    let sorted = conn
        .request(
            json!({"type": "sort-capture", "capture": entry.id, "rev": entry.rev,
                        "action": "work"}),
        )
        .await;
    match sorted.first() {
        Some(Message::CaptureSaved { work: Some(w), .. }) => (**w).clone(),
        other => panic!("{other:?}"),
    }
}

async fn ask(conn: &mut Conn, work: &WorkItem) -> AssistantRequest {
    let answer = conn
        .request(
            json!({"type": "request-assistant", "work": work.id, "rev": work.rev,
                        "scope": "research"}),
        )
        .await;
    match answer.as_slice() {
        [
            Message::WorkSaved {
                request: Some(q), ..
            },
            Message::Result { error: None, .. },
        ] => (**q).clone(),
        other => panic!("request-assistant: {other:?}"),
    }
}

async fn items(conn: &mut Conn) -> (Vec<InboxItem>, Vec<AssistantRequest>) {
    let answer = conn.request(json!({"type": "list-inbox"})).await;
    match answer.first() {
        Some(Message::InboxList {
            items, requests, ..
        }) => (items.clone(), requests.clone()),
        other => panic!("{other:?}"),
    }
}

/// The request's `result` item, once the Result is stored and the tick made it.
async fn until_result_item(conn: &mut Conn, request: &str) -> InboxItem {
    let deadline = Instant::now() + RUN_WAIT;
    loop {
        let (items, requests) = items(conn).await;
        let q = requests.iter().find(|q| q.id == request).unwrap();
        let item = items.into_iter().find(|i| {
            i.kind == ItemKind::Result && i.result.as_ref().is_some_and(|r| r.request == request)
        });
        if let (Some(item), Delivery::Resulted) = (item, q.delivery) {
            return item;
        }
        assert!(Instant::now() < deadline, "no result item: {q:?}");
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
}

async fn answer(conn: &mut Conn, item: &str, action: &str) -> Vec<Message> {
    conn.request(json!({"type": "update-inbox", "entry": item, "action": action}))
        .await
}

/// The Task an approve started, and the item it answered.
fn started(answer: &[Message]) -> (TaskRecord, InboxItem) {
    match answer {
        [
            Message::TaskSaved { record, .. },
            Message::InboxSaved { item: Some(i), .. },
            Message::Result { error: None, .. },
        ] => (record.clone(), (**i).clone()),
        other => panic!("approve: {other:?}"),
    }
}

fn assert_no_marks(daemon: &Daemon) {
    let log = daemon.log();
    for mark in [GOAL_MARK, TITLE_MARK] {
        assert!(!log.contains(mark), "the daemon log holds {mark}:\n{log}");
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_leads_proposed_task_starts_once_in_plan_mode() {
    let h = Host::new("lead");
    h.steer(&format!("first.{LEAD}.json"), &proposing());
    let mut daemon = h.daemon().await;
    let mut conn = daemon.connect().await;
    let before = h.server.layout();
    let work = work_item(&mut conn, "c-proposal-1").await;
    let q = ask(&mut conn, &work).await;
    let run = q.run.clone().unwrap();
    let item = until_result_item(&mut conn, &q.id).await;

    // The run's TASK.md asked for a Task instead of a refusal, naming the repo.
    let dir = PathBuf::from(h.record(&run)["dir"].as_str().unwrap().to_owned());
    let task_md = std::fs::read_to_string(dir.join(".muxa/TASK.md")).unwrap();
    assert!(
        task_md.contains("\"task\": optional, one Task to start"),
        "{task_md}"
    );
    assert!(task_md.contains("repos (app)"), "{task_md}");

    // The item offers the lead's Task: approve is Start Task.
    let p = item
        .result
        .as_ref()
        .and_then(|r| r.task.clone())
        .expect("a proposal");
    assert_eq!(
        (
            p.workspace.as_str(),
            p.repo.as_deref(),
            p.step,
            p.worker.as_deref()
        ),
        ("muxa", Some("app"), Some(1), Some(LEAD))
    );
    assert_eq!(
        item.answers,
        [
            InboxAction::Approve,
            InboxAction::Reply,
            InboxAction::Dismiss,
            InboxAction::Later
        ]
    );
    assert_eq!(
        item.question,
        format!("{LEAD} proposes a Task: Fix the crash {TITLE_MARK}")
    );
    assert!(h.proposed_tasks().is_empty());

    let agents = h.argv().len();
    let (task, answered) = started(&answer(&mut conn, &item.id, "approve").await);
    assert_eq!(task.title, format!("Fix the crash {TITLE_MARK}"));
    assert_eq!(task.work.as_deref(), Some(work.id.as_str()));
    assert_eq!(task.worker, None, "the workspace's default Task worker");
    assert_eq!(
        task.repo.as_deref().map(Path::new),
        Some(h.base.join("muxa/app").as_path())
    );
    assert!(
        task.goal
            .starts_with(&format!("Guard the empty summary. {GOAL_MARK}\n\n"))
    );
    assert!(task.goal.contains("Plan first"), "{}", task.goal);
    assert_eq!(
        task.goal.lines().next_back(),
        Some(format!("muxa-task-proposal: {} v1 s1", q.id).as_str())
    );
    assert_eq!(
        (answered.state, answered.answer, answered.task.as_deref()),
        (
            ItemState::Answered,
            Some(InboxAction::Approve),
            Some(task.id.as_str())
        )
    );
    assert!(
        answered
            .summary
            .as_deref()
            .is_some_and(|s| s.starts_with(&format!("Started Task {}: ", task.id))),
        "{answered:?}"
    );
    let argv = h.until_planning(agents).await;
    assert!(argv.ends_with(PLAN_FLAG), "the Task's agent args: {argv}");

    // A second approve, also start-task, starts nothing new.
    let (again, _) = started(&answer(&mut conn, &item.id, "approve").await);
    assert_eq!(again.id, task.id);
    let (again, _) = started(&answer(&mut conn, &item.id, "start-task").await);
    assert_eq!(again.id, task.id);
    assert_eq!(h.proposed_tasks().len(), 1);
    // The Work item lists the Task.
    let listed = conn
        .request(json!({"type": "get-work", "work": work.id}))
        .await;
    match listed.first() {
        Some(Message::WorkDetail { entry, .. }) => {
            assert!(entry.tasks.contains(&task.id), "{entry:?}")
        }
        other => panic!("{other:?}"),
    }
    // Staff runs started without the plan flag: it is for proposed Tasks only.
    assert!(!h.argv()[..agents].iter().any(|a| a.contains(PLAN_FLAG)));
    assert_no_marks(&daemon);
    let after = h.server.layout();
    for line in before.lines() {
        assert!(after.lines().any(|l| l == line), "{line} changed:\n{after}");
    }
    daemon.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn the_secretarys_proposal_is_an_item_that_starts_a_task() {
    let h = Host::new("secretary");
    let mut daemon = h.daemon().await;
    let mut conn = daemon.connect().await;
    let work = work_item(&mut conn, "c-proposal-2").await;
    let propose = json!({"type": "propose-task", "workspace": "muxa", "repo": "app",
                         "title": format!("Fix the crash {TITLE_MARK}"),
                         "goal": format!("Guard the empty summary. {GOAL_MARK}"),
                         "work": work.id});
    let made = conn.request(propose.clone()).await;
    let item = match made.as_slice() {
        [
            Message::InboxSaved { item: Some(i), .. },
            Message::Result { error: None, .. },
        ] => (**i).clone(),
        other => panic!("propose-task: {other:?}"),
    };
    assert_eq!(item.kind, ItemKind::NewTask);
    assert_eq!(
        item.answers,
        [
            InboxAction::Approve,
            InboxAction::Reply,
            InboxAction::Dismiss,
            InboxAction::Later
        ]
    );
    assert_eq!(
        item.new_task.as_ref().and_then(|t| t.work.as_deref()),
        Some(work.id.as_str())
    );
    // The same proposal again returns the open item.
    let again = conn.request(propose).await;
    assert!(
        matches!(again.first(), Some(Message::InboxSaved { item: Some(i), .. }) if i.id == item.id),
        "{again:?}"
    );
    // A repo the workspace does not have is refused before any item.
    let wrong = conn
        .request(
            json!({"type": "propose-task", "workspace": "muxa", "repo": "nope",
                        "title": "t", "goal": "g"}),
        )
        .await;
    assert!(
        matches!(wrong.last(), Some(Message::Result { error: Some(_), .. })),
        "{wrong:?}"
    );

    let agents = h.argv().len();
    let (task, answered) = started(&answer(&mut conn, &item.id, "approve").await);
    assert_eq!(task.work.as_deref(), Some(work.id.as_str()));
    assert!(
        task.goal.contains("The secretary proposed this Task"),
        "{}",
        task.goal
    );
    assert_eq!(answered.task.as_deref(), Some(task.id.as_str()));
    let argv = h.until_planning(agents).await;
    assert!(argv.ends_with(PLAN_FLAG), "{argv}");
    let (twice, _) = started(&answer(&mut conn, &item.id, "approve").await);
    assert_eq!(twice.id, task.id);
    assert_eq!(h.proposed_tasks().len(), 1);
    assert_no_marks(&daemon);
    daemon.shutdown().await;
}
