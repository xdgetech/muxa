//! External writes only when asked (ADR 0052) end to end: a real daemon on
//! a throwaway tmux server runs a staff member, the fake agent of
//! `data/assistant-agent.py`, whose Result proposes writes. Each write
//! becomes one `write` Inbox item; approve makes one Task of the writer,
//! never of a reading member, also when sent twice or after a crash; a
//! workspace without writes refuses them; `max_writes_per_day` holds an
//! item open. Actions, targets, contents and reasons never reach the log.
#![cfg(feature = "tmux-integration")]

mod common;

use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use common::Server;
use common::daemon::{Conn, Daemon};
use muxa_protocol::{
    AssistantRequest, Delivery, ErrorCode, InboxAction, InboxItem, ItemKind, ItemState, Message,
    TaskRecord, WorkItem,
};
use serde_json::{Value, json};

const AGENT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/data/assistant-agent.py");
const DELIVERY: &str = "lisk-delivery";
const RESEARCH: &str = "lisk-research";
const WRITER: &str = "lisk-writer";
/// The write's parts, which must never reach the daemon's log.
const MARKS: [&str; 4] = [
    "action-mark-3d91",
    "target-mark-a07c",
    "content-mark-55e2",
    "reason-mark-c4b8",
];
const RUN_WAIT: Duration = Duration::from_secs(120);

struct Host {
    server: Server,
    base: PathBuf,
}

impl Drop for Host {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.base);
    }
}

impl Host {
    /// Workspace `lisk` with a staff of two, the writer, and `extra` in its
    /// `[workspace.assistant]`; `host` in `[assistant]`.
    fn new(name: &str, extra: &str, host: &str) -> Host {
        let base =
            std::env::temp_dir().join(format!("muxa-writes-it-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        for d in ["lisk", "state", "steer", "bin", "claude"] {
            std::fs::create_dir_all(base.join(d)).unwrap();
        }
        let base = base.canonicalize().unwrap();
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
                "[assistant]\nenabled = true\n{host}\n\
                 [[workspace]]\nname = \"lisk\"\nroot = \"{root}\"\n\
                 [workspace.agent]\nenv = {{ PATH = \"{bin}:{path}\", ASSISTANT_STEER = \"{steer}\", \
                 CLAUDE_CONFIG_DIR = \"{claude}\" }}\n\
                 [workspace.assistant]\ndelegate = true\nmax_runs_per_day = 20\nmax_open_runs = 3\n\
                 staff = [\"{DELIVERY}\", \"{RESEARCH}\"]\nwriter = \"{WRITER}\"\n{extra}\n\
                 {}{}{}",
                worker(DELIVERY, "Delivery: release notes, read-only."),
                worker(RESEARCH, "Research: sources, read-only."),
                worker(WRITER, "Writer: posts approved text."),
                root = base.join("lisk").display(),
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

    fn turns(&self) -> Vec<String> {
        std::fs::read_to_string(self.base.join("steer/turns.log"))
            .unwrap_or_default()
            .lines()
            .map(str::to_owned)
            .collect()
    }

    fn record(&self, id: &str) -> Value {
        let text = std::fs::read_to_string(self.base.join(format!("state/{id}.json"))).unwrap();
        serde_json::from_str(&text).unwrap()
    }

    /// Every Task record the writer runs.
    fn writer_tasks(&self) -> Vec<Value> {
        std::fs::read_dir(self.base.join("state"))
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().starts_with("t-"))
            .filter_map(|e| std::fs::read_to_string(e.path()).ok())
            .filter_map(|t| serde_json::from_str::<Value>(&t).ok())
            .filter(|r| r["worker"] == WRITER)
            .collect()
    }

    /// Undoes what approve recorded on `item` in `run`'s item file, as a
    /// crash between the writer's Task and its record would leave it.
    fn forget_approval(&self, run: &str, item: &str) {
        let path = self.base.join(format!("state/items/{run}.json"));
        let mut file: Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        for i in file["items"].as_array_mut().unwrap() {
            if i["id"] == item {
                let i = i.as_object_mut().unwrap();
                for field in ["answer", "answeredAt", "task"] {
                    i.remove(field);
                }
                i.insert("state".into(), json!("open"));
                i.insert("answers".into(), json!(["approve", "reject", "later"]));
            }
        }
        file.as_object_mut().unwrap().remove("approvals");
        std::fs::write(&path, file.to_string()).unwrap();
    }
}

fn result(summary: &str) -> Value {
    json!({"version": 1, "request": "@", "requestVersion": 0, "summary": summary})
}

/// A result proposing `n` writes, each carrying the marks.
fn proposing(n: usize) -> Value {
    let writes: Vec<Value> = (0..n)
        .map(|i| {
            json!({"action": format!("Post note {i} {}", MARKS[0]),
                   "target": format!("slack:#release-{i}-{}", MARKS[1]),
                   "content": format!("Release 2.4 is out.\n\n- {} {i}\n", MARKS[2]),
                   "reason": format!("The user asked. {}", MARKS[3])})
        })
        .collect();
    let mut v = result("The note is ready.");
    v["writes"] = json!(writes);
    v
}

async fn work_item(conn: &mut Conn, client: &str) -> WorkItem {
    let saved = conn
        .request(
            json!({"type": "capture", "clientId": client, "text": "Ship release 2.4",
                        "workspace": "lisk"}),
        )
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
                        "scope": "draft", "worker": DELIVERY}),
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

async fn listed(conn: &mut Conn) -> (Vec<InboxItem>, Vec<AssistantRequest>) {
    let answer = conn.request(json!({"type": "list-inbox"})).await;
    match answer.first() {
        Some(Message::InboxList {
            items, requests, ..
        }) => (items.clone(), requests.clone()),
        other => panic!("{other:?}"),
    }
}

async fn until_resulted(conn: &mut Conn, h: &Host, id: &str) -> AssistantRequest {
    let deadline = Instant::now() + RUN_WAIT;
    loop {
        let (_, requests) = listed(conn).await;
        let q = requests.into_iter().find(|q| q.id == id).unwrap();
        if q.delivery == Delivery::Resulted {
            return q;
        }
        assert!(
            Instant::now() < deadline,
            "not resulted: {q:?}; turns: {:?}",
            h.turns()
        );
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
}

/// The request's `write` items once `n` exist, and each write links one.
async fn until_items(
    conn: &mut Conn,
    request: &str,
    n: usize,
) -> (Vec<InboxItem>, AssistantRequest) {
    let deadline = Instant::now() + RUN_WAIT;
    loop {
        let (items, requests) = listed(conn).await;
        let mine: Vec<InboxItem> = items
            .into_iter()
            .filter(|i| i.kind == ItemKind::Write)
            .filter(|i| i.write.as_ref().is_some_and(|w| w.request == request))
            .collect();
        let q = requests.into_iter().find(|q| q.id == request).unwrap();
        let linked = q.results[0].writes.iter().all(|w| w.item.is_some());
        if mine.len() == n && linked {
            return (mine, q);
        }
        assert!(Instant::now() < deadline, "{} write items", mine.len());
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
}

async fn answer(conn: &mut Conn, item: &str, action: &str) -> Vec<Message> {
    conn.request(json!({"type": "update-inbox", "entry": item, "action": action}))
        .await
}

/// The writer's Task an approve made, and the item it answered.
fn approved(answer: &[Message]) -> (TaskRecord, InboxItem) {
    match answer {
        [
            Message::TaskSaved { record, .. },
            Message::InboxSaved { item: Some(i), .. },
            Message::Result { error: None, .. },
        ] => (record.clone(), (**i).clone()),
        other => panic!("approve: {other:?}"),
    }
}

fn error_of(answer: &[Message]) -> (ErrorCode, String) {
    match answer.last() {
        Some(Message::Result { error: Some(e), .. }) => (e.code, e.message.clone()),
        other => panic!("expected an error: {other:?}"),
    }
}

fn assert_no_marks(daemon: &Daemon) {
    let log = daemon.log();
    for mark in MARKS {
        assert!(!log.contains(mark), "the daemon log holds {mark}:\n{log}");
    }
}

fn assert_layout_kept(before: &str, h: &Host) {
    let after = h.server.layout();
    for line in before.lines() {
        assert!(after.lines().any(|l| l == line), "{line} changed:\n{after}");
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn an_approved_write_is_one_task_of_the_writer() {
    let h = Host::new("approve", "", "");
    h.steer(&format!("first.{DELIVERY}.json"), &proposing(2));
    let mut daemon = h.daemon().await;
    let mut conn = daemon.connect().await;
    let before = h.server.layout();
    let work = work_item(&mut conn, "c-writes-1").await;
    let q = ask(&mut conn, &work).await;
    let run = q.run.clone().unwrap();
    until_resulted(&mut conn, &h, &q.id).await;

    // Two writes make two items, each with the write whole.
    let (items, q) = until_items(&mut conn, &q.id, 2).await;
    let writes = &q.results[0].writes;
    for w in writes {
        let item = items
            .iter()
            .find(|i| i.write.as_ref().is_some_and(|p| p.hash == w.hash))
            .expect("each write has its item");
        let p = item.write.as_ref().unwrap();
        assert_eq!(p.content, w.content);
        assert_eq!(
            item.answers,
            [
                InboxAction::Approve,
                InboxAction::Reject,
                InboxAction::Later
            ]
        );
        assert_eq!(p.worker.as_deref(), Some(DELIVERY));
        assert!(
            item.question
                .starts_with(&format!("{DELIVERY} proposes to write to slack:#release-"))
        );
        assert_eq!(item.summary.as_deref(), w.reason.as_deref());
        assert_eq!(w.item.as_deref(), Some(item.id.as_str()));
    }
    // The run's TASK.md told the member how to propose them.
    let dir = PathBuf::from(h.record(&run)["dir"].as_str().unwrap().to_owned());
    let task_md = std::fs::read_to_string(dir.join(".muxa/TASK.md")).unwrap();
    assert!(
        task_md.contains("\"writes\": optional, at most 3"),
        "{task_md}"
    );

    // Approve makes one Task of the writer, with the content unchanged and the marker last.
    let first = items
        .iter()
        .find(|i| i.write.as_ref().unwrap().hash == writes[0].hash)
        .unwrap();
    let (task, item) = approved(&answer(&mut conn, &first.id, "approve").await);
    let w = &writes[0];
    assert_eq!(task.worker.as_deref(), Some(WRITER));
    assert_eq!(task.title, w.action);
    assert_eq!(task.work.as_deref(), Some(work.id.as_str()));
    assert!(
        task.goal.contains(&format!("\n\n{}\n\n", w.content)),
        "{}",
        task.goal
    );
    assert_eq!(
        task.goal.lines().next_back(),
        Some(format!("muxa-write: {} {}", first.id, w.hash).as_str())
    );
    assert_eq!(
        (item.state, item.answer, item.task.as_deref()),
        (
            ItemState::Answered,
            Some(InboxAction::Approve),
            Some(task.id.as_str())
        )
    );
    // A second approve makes nothing new.
    let (again, _) = approved(&answer(&mut conn, &first.id, "approve").await);
    assert_eq!(again.id, task.id);
    // So does one after a crash between the Task and its record.
    h.forget_approval(&run, &first.id);
    let (recovered, item) = approved(&answer(&mut conn, &first.id, "approve").await);
    assert_eq!(
        (recovered.id.as_str(), item.task.as_deref()),
        (task.id.as_str(), Some(task.id.as_str()))
    );
    assert_eq!(h.writer_tasks().len(), 1);

    // The writer runs it; the reading members never see the write.
    let deadline = Instant::now() + RUN_WAIT;
    while !h
        .turns()
        .iter()
        .any(|t| t.starts_with(&format!("{WRITER}: ")))
    {
        assert!(
            Instant::now() < deadline,
            "the writer never ran: {:?}",
            h.turns()
        );
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    let readers = h.turns();
    assert!(
        readers
            .iter()
            .filter(|t| t.starts_with(&format!("{DELIVERY}: "))
                || t.starts_with(&format!("{RESEARCH}: ")))
            .all(|t| !t.contains("muxa-write")),
        "{readers:?}"
    );
    assert_eq!(h.record(&task.id)["worker"], WRITER);

    // Reject makes no Task; the Result shows the Task and both items.
    let second = items.iter().find(|i| i.id != first.id).unwrap();
    let rejected = answer(&mut conn, &second.id, "reject").await;
    let Some(Message::InboxSaved { item: Some(r), .. }) = rejected.first() else {
        panic!("{rejected:?}")
    };
    assert_eq!(
        (r.answer, r.task.as_ref()),
        (Some(InboxAction::Reject), None)
    );
    assert_eq!(h.writer_tasks().len(), 1);
    let (_, requests) = listed(&mut conn).await;
    let q = requests.into_iter().find(|x| x.id == q.id).unwrap();
    let tasks: Vec<Option<&str>> = q.results[0]
        .writes
        .iter()
        .map(|w| w.task.as_deref())
        .collect();
    assert_eq!(tasks, [Some(task.id.as_str()), None]);
    let listed_work = conn
        .request(json!({"type": "get-work", "work": work.id}))
        .await;
    let Some(Message::WorkDetail { entry, .. }) = listed_work.first() else {
        panic!("{listed_work:?}")
    };
    assert!(entry.tasks.contains(&task.id), "{entry:?}");

    assert_layout_kept(&before, &h);
    assert_no_marks(&daemon);
    daemon.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn none_refuses_writes_and_the_day_limit_holds_an_item_open() {
    // `writes = "none"`: the check refuses them and says where they go.
    let h = Host::new("none", "writes = \"none\"", "");
    h.steer(&format!("first.{DELIVERY}.json"), &proposing(1));
    h.steer(
        &format!("fixed.{DELIVERY}.json"),
        &result("Suggested in next."),
    );
    let mut daemon = h.daemon().await;
    let mut conn = daemon.connect().await;
    let work = work_item(&mut conn, "c-writes-none").await;
    let q = ask(&mut conn, &work).await;
    let run = q.run.clone().unwrap();
    let done = until_resulted(&mut conn, &h, &q.id).await;
    assert!(done.results[0].writes.is_empty());
    let dir = PathBuf::from(h.record(&run)["dir"].as_str().unwrap().to_owned());
    let log = std::fs::read_to_string(dir.join(".muxa/checks/1.log")).unwrap();
    assert!(log.contains("takes no write proposals"), "{log}");
    for mark in MARKS {
        assert!(!log.contains(mark), "{log}");
    }
    let task_md = std::fs::read_to_string(dir.join(".muxa/TASK.md")).unwrap();
    assert!(
        task_md.contains("go into \"next\" as suggestions"),
        "{task_md}"
    );
    let (items, _) = listed(&mut conn).await;
    assert!(items.iter().all(|i| i.kind != ItemKind::Write));
    assert_no_marks(&daemon);
    daemon.shutdown().await;
    drop(h);

    // One write a day: the second approve is `conflict` and its item stays open.
    let h = Host::new("limit", "", "max_writes_per_day = 1");
    h.steer(&format!("first.{DELIVERY}.json"), &proposing(2));
    let mut daemon = h.daemon().await;
    let mut conn = daemon.connect().await;
    let before = h.server.layout();
    let work = work_item(&mut conn, "c-writes-limit").await;
    let q = ask(&mut conn, &work).await;
    until_resulted(&mut conn, &h, &q.id).await;
    let (items, _) = until_items(&mut conn, &q.id, 2).await;
    approved(&answer(&mut conn, &items[0].id, "approve").await);
    let (code, message) = error_of(&answer(&mut conn, &items[1].id, "approve").await);
    assert_eq!(code, ErrorCode::Conflict);
    assert!(message.contains("max_writes_per_day"), "{message}");
    let (after, _) = listed(&mut conn).await;
    let held = after.iter().find(|i| i.id == items[1].id).unwrap();
    assert_eq!((held.state, held.task.as_ref()), (ItemState::Open, None));
    assert_eq!(h.writer_tasks().len(), 1);

    assert_layout_kept(&before, &h);
    assert_no_marks(&daemon);
    daemon.shutdown().await;
}
