//! Routines (ADR 0026) end to end: a real daemon on a throwaway tmux server
//! runs a Routine with the fake agent of `data/routine-agent.py`. Run-now,
//! a failed output check and its fixed prompt, the publish, a retry that
//! stores nothing twice, and an approval that creates exactly one Task.
//! Without a daemon, a bridge lists Routines but does not run them.
#![cfg(feature = "tmux-integration")]

mod common;

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use common::daemon::{Conn, Daemon};
use common::{Bridge, Server};
use muxa_protocol::{
    Disposition, ErrorCode, InboxEntry, InboxEntryKind, Message, RoutineInfo, TaskRecord,
    TaskStatus, work_level,
};
use serde_json::{Value, json};

const AGENT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/data/routine-agent.py");
/// Content that must never reach the daemon's log (invariant 5).
const MARKS: [&str; 4] = [
    "instruction-mark-51c0",
    "title-mark-9a1e",
    "body-mark-73bd",
    "state-mark-e04f",
];
const FAIL_PROMPT: &str = "did not pass the check";
const RUN_WAIT: Duration = Duration::from_secs(120);

struct Host {
    base: PathBuf,
}

impl Host {
    fn new(name: &str) -> Host {
        Self::with(name, "")
    }

    /// `extra` is added to the workspace's `.muxa/routines.toml`.
    fn with(name: &str, extra: &str) -> Host {
        let base =
            std::env::temp_dir().join(format!("muxa-routines-it-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        for d in ["root/.muxa", "state", "steer"] {
            std::fs::create_dir_all(base.join(d)).unwrap();
        }
        let base = base.canonicalize().unwrap();
        let muxa = base.join("root/.muxa");
        std::fs::write(
            muxa.join("brief.md"),
            format!("Read the sources. {}\n", MARKS[0]),
        )
        .unwrap();
        std::fs::write(
            base.join("workspaces.toml"),
            format!(
                "[[workspace]]\nname = \"w\"\nroot = \"{root}\"\n\
                 [workspace.routines]\nenabled = true\n\
                 [[worker]]\nname = \"Cx\"\nagent = \"codex\"\n",
                root = base.join("root").display(),
            ),
        )
        .unwrap();
        std::fs::write(
            muxa.join("routines.toml"),
            format!(
                "[[routine]]\nname = \"Brief\"\ninstructions = \"brief.md\"\n\
                 project = \"Ops\"\nenabled = true\n[routine.output]\nitems = 5\n\
                 [[routine]]\nname = \"Off\"\ninstructions = \"brief.md\"\n\
                 at = \"09:00\"\ntimezone = \"UTC\"\nenabled = false\n\
                 [[routine]]\nname = \"Codex\"\ninstructions = \"brief.md\"\n\
                 worker = \"Cx\"\nenabled = true\n{extra}"
            ),
        )
        .unwrap();
        Host { base }
    }

    fn steer(&self, name: &str, value: &Value) {
        std::fs::write(self.base.join("steer").join(name), value.to_string()).unwrap();
    }

    fn flags(&self) -> Vec<String> {
        vec![
            "--state-dir".into(),
            self.base.join("state").display().to_string(),
            "--workspaces".into(),
            self.base.join("workspaces.toml").display().to_string(),
            "--task-launcher".into(),
            "builtin".into(),
            "--task-command".into(),
            format!("python3 {AGENT} {}", self.base.join("steer").display()),
        ]
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
}

impl Drop for Host {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.base);
    }
}

fn output(items: Value) -> Value {
    json!({"version": 1, "items": items, "state": {"cursor": MARKS[3]}})
}

fn briefing() -> Value {
    json!({"kind": "briefing", "key": "src:1", "title": format!("Thread {}", MARKS[1]),
           "body": format!("Moved. {}", MARKS[2]),
           "links": [{"url": "https://example.com/t/1"}]})
}

fn proposal() -> Value {
    json!({"kind": "proposal", "key": "src:2", "title": "Close ENG-12", "body": "Stale.",
           "action": "Close the issue", "target": "linear:ENG-12", "content": "Closing."})
}

async fn routines(conn: &mut Conn) -> Vec<RoutineInfo> {
    let answer = conn.request(json!({"type": "list-routines"})).await;
    match answer.as_slice() {
        [
            Message::RoutineList { entries, .. },
            Message::Result { error: None, .. },
        ] => entries.clone(),
        other => panic!("list-routines: {other:?}"),
    }
}

async fn brief(conn: &mut Conn) -> RoutineInfo {
    routines(conn)
        .await
        .into_iter()
        .find(|r| r.name == "Brief")
        .expect("Brief")
}

async fn inbox(conn: &mut Conn) -> Vec<InboxEntry> {
    match conn.request(json!({"type": "list-inbox"})).await.as_slice() {
        [
            Message::InboxList { entries, .. },
            Message::Result { error: None, .. },
        ] => entries.clone(),
        other => panic!("list-inbox: {other:?}"),
    }
}

async fn tasks(conn: &mut Conn) -> Vec<TaskRecord> {
    match conn.request(json!({"type": "list-tasks"})).await.as_slice() {
        [
            Message::TaskList { entries, .. },
            Message::Result { error: None, .. },
        ] => entries.clone(),
        other => panic!("list-tasks: {other:?}"),
    }
}

async fn run_now(conn: &mut Conn) -> RoutineInfo {
    let answer = conn
        .request(json!({"type": "run-routine", "workspace": "w", "routine": "Brief"}))
        .await;
    match answer.as_slice() {
        [
            Message::RoutineSaved { entry, .. },
            Message::Result { error: None, .. },
        ] => entry.clone(),
        other => panic!("run-routine: {other:?}"),
    }
}

/// Waits until the open run is gone, and returns the Routine.
async fn ended(conn: &mut Conn, h: &Host) -> RoutineInfo {
    let deadline = Instant::now() + RUN_WAIT;
    loop {
        let r = brief(conn).await;
        if r.running.is_none() {
            return r;
        }
        assert!(
            Instant::now() < deadline,
            "the run did not end: {r:?}; turns: {:?}",
            h.turns()
        );
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
}

fn error_of(answer: &[Message]) -> (ErrorCode, String) {
    match answer.last() {
        Some(Message::Result { error: Some(e), .. }) => (e.code, e.message.clone()),
        other => panic!("expected an error: {other:?}"),
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_run_publishes_once_and_an_approval_creates_one_task() {
    let server = Server::start();
    let h = Host::new("daemon");
    h.steer(
        "first.json",
        &output(json!([{"kind": "briefing", "key": "k", "title": "t",
        "links": [{"url": "javascript:alert(1)"}]}])),
    );
    h.steer("fixed.json", &output(json!([briefing(), proposal()])));
    let flags = h.flags();
    let flags: Vec<&str> = flags.iter().map(String::as_str).collect();
    let mut daemon = Daemon::start(&server, &flags).await;
    let mut conn = daemon.connect().await;
    let Message::Hello { capabilities, .. } = &conn.hello else {
        panic!("{:?}", conn.hello)
    };
    assert_eq!(work_level(capabilities), 1);
    let before = server.layout();

    // Disabled and Codex Routines never start.
    let off = conn
        .request(json!({"type": "run-routine", "workspace": "w", "routine": "Off"}))
        .await;
    assert_eq!(error_of(&off).0, ErrorCode::BadRequest);
    let codex = conn
        .request(json!({"type": "run-routine", "workspace": "w", "routine": "Codex"}))
        .await;
    let (code, message) = error_of(&codex);
    assert_eq!(code, ErrorCode::Unsupported);
    assert!(message.contains("Codex"), "{message}");

    let started = run_now(&mut conn).await;
    let running = started.running.expect("an open run");
    assert!(running.occurrence.starts_with("now-"));
    let run = running.task.expect("the run's record");
    let again = conn
        .request(json!({"type": "run-routine", "workspace": "w", "routine": "Brief"}))
        .await;
    assert_eq!(error_of(&again).0, ErrorCode::Conflict, "one open run");
    assert!(tasks(&mut conn).await.is_empty(), "a run is not a Task");

    let done = ended(&mut conn, &h).await;
    let success = done.last_success.expect("a publish");
    assert_eq!(
        (success.task.as_str(), success.published),
        (run.as_str(), 2)
    );
    let turns = h.turns();
    assert_eq!(
        turns.iter().filter(|t| t.contains(FAIL_PROMPT)).count(),
        1,
        "{turns:?}"
    );
    // The loop moves the record to done on the tick after the publish.
    let deadline = Instant::now() + Duration::from_secs(20);
    while h.record(&run)["status"] != "done" {
        assert!(Instant::now() < deadline, "{}", h.record(&run));
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
    let record = h.record(&run);
    assert_eq!(record["kind"], "routine-run");
    assert_eq!(record["loop"]["state"], "passed");
    let dir = PathBuf::from(record["dir"].as_str().unwrap());
    assert!(dir.starts_with(h.base.join("root/routines")));
    let log = std::fs::read_to_string(dir.join(".muxa/checks/1.log")).unwrap();
    assert!(log.contains("links[0].url"), "{log}");
    assert!(
        std::fs::read_to_string(dir.join(".muxa/TASK.md"))
            .unwrap()
            .contains(MARKS[0])
    );
    let entries = inbox(&mut conn).await;
    assert_eq!(entries.len(), 2);
    assert!(
        entries
            .iter()
            .all(|e| e.disposition == Disposition::Open && e.run == run)
    );
    // ADR 0037: the published run's session closes; its folder stays for now.
    let tagged = |server: &Server, task: &str| {
        server
            .tmux(&["list-sessions", "-F", "#{@muxa_task}"])
            .lines()
            .any(|t| t == task)
    };
    let deadline = Instant::now() + Duration::from_secs(20);
    while tagged(&server, &run) {
        assert!(
            Instant::now() < deadline,
            "the published run's session stays"
        );
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
    assert!(dir.is_dir(), "the last published run keeps its folder");

    // A second run repeats both keys: nothing new, and it got the committed state.
    h.steer("first.json", &output(json!([briefing(), proposal()])));
    run_now(&mut conn).await;
    let second = ended(&mut conn, &h).await.last_success.expect("a publish");
    assert_ne!(second.task, run);
    assert_eq!(second.published, 0);
    // The next publish removes the older run's folder.
    let deadline = Instant::now() + Duration::from_secs(20);
    while dir.is_dir() {
        assert!(Instant::now() < deadline, "the older run's folder stays");
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
    let second_dir = PathBuf::from(h.record(&second.task)["dir"].as_str().unwrap());
    assert!(
        second_dir.is_dir(),
        "the newest published run keeps its folder"
    );
    assert_eq!(inbox(&mut conn).await.len(), 2);
    let seen = std::fs::read_to_string(h.base.join("steer/seen-state-0.json")).unwrap();
    assert!(seen.contains(MARKS[3]), "{seen}");
    assert!(tasks(&mut conn).await.is_empty());

    // Approve twice: one Task, with the proposal and its hash in its goal.
    let proposal = entries
        .iter()
        .find(|e| e.kind == InboxEntryKind::Proposal)
        .unwrap();
    let approve = json!({"type": "update-inbox", "entry": proposal.id, "action": "approve"});
    let first = conn.request(approve.clone()).await;
    let Some(Message::InboxSaved {
        entry: Some(entry),
        item: Some(item),
        ..
    }) = first.first()
    else {
        panic!("{first:?}")
    };
    assert_eq!(entry.disposition, Disposition::Approved);
    // A Routine's entry is also answered as an item (ADR 0033).
    assert_eq!(item.id, entry.id);
    assert_eq!(item.task, entry.task);
    let task = entry.task.clone().expect("a Task");
    let second = conn.request(approve).await;
    let Some(Message::InboxSaved {
        entry: Some(again), ..
    }) = second.first()
    else {
        panic!("{second:?}")
    };
    assert_eq!(again.task.as_deref(), Some(task.as_str()));
    let listed = tasks(&mut conn).await;
    assert_eq!(listed.len(), 1, "{listed:?}");
    assert_eq!(listed[0].id, task);
    assert_eq!(listed[0].project.as_deref(), Some("Ops"));
    let hash = &proposal.proposal.as_ref().unwrap().hash;
    assert!(listed[0].goal.contains("\n\nClosing.\n\n") && listed[0].goal.contains(hash.as_str()));
    assert_eq!(listed[0].status, TaskStatus::Open);

    // Dismiss and later on the briefing.
    let briefing = entries
        .iter()
        .find(|e| e.kind == InboxEntryKind::Briefing)
        .unwrap();
    let bad = conn
        .request(json!({"type": "update-inbox", "entry": briefing.id, "action": "approve"}))
        .await;
    assert_eq!(error_of(&bad).0, ErrorCode::BadRequest);
    // A device opening the briefing marks it read once; it stays open.
    let mut read_at = None;
    for _ in 0..2 {
        let read = conn
            .request(json!({"type": "update-inbox", "entry": briefing.id, "action": "read"}))
            .await;
        let Some(Message::InboxSaved {
            entry: Some(entry),
            item: Some(item),
            ..
        }) = read.first()
        else {
            panic!("read answered {read:?}");
        };
        assert_eq!(entry.disposition, Disposition::Open);
        assert!(entry.read_at.is_some() && item.read_at == entry.read_at);
        assert!(read_at.is_none() || read_at == entry.read_at);
        read_at = entry.read_at;
    }
    let later = conn
        .request(
            json!({"type": "update-inbox", "entry": briefing.id, "action": "later",
            "until": 4_000_000_000u64}),
        )
        .await;
    assert!(
        matches!(later.first(), Some(Message::InboxSaved { entry: Some(entry), .. })
        if entry.disposition == Disposition::Later)
    );
    let dismissed = conn
        .request(json!({"type": "update-inbox", "entry": briefing.id, "action": "dismiss"}))
        .await;
    assert!(
        matches!(dismissed.first(), Some(Message::InboxSaved { entry: Some(entry), .. })
        if entry.disposition == Disposition::Dismissed)
    );

    // New sessions run the work; the windows that were there do not change.
    let after = server.layout();
    for line in before.lines() {
        assert!(after.lines().any(|l| l == line), "{line} changed:\n{after}");
    }
    let daemon_log = daemon.log();
    for mark in MARKS {
        assert!(
            !daemon_log.contains(mark),
            "the daemon log holds {mark}:\n{daemon_log}"
        );
    }
    daemon.shutdown().await;
}

/// An agent that sits on a dialog gets no start prompt: the handoff gives up
/// after 30 s. The run fails then, and when the user answers the dialog the
/// agent's first finished turn gets no fail prompt.
#[tokio::test(flavor = "multi_thread")]
async fn a_run_whose_start_prompt_was_not_typed_fails_without_a_fail_prompt() {
    let server = Server::start();
    let h = Host::new("not-sent");
    h.steer("hold", &json!(true));
    let flags = h.flags();
    let flags: Vec<&str> = flags.iter().map(String::as_str).collect();
    let mut daemon = Daemon::start(&server, &flags).await;
    let mut conn = daemon.connect().await;

    let run = run_now(&mut conn)
        .await
        .running
        .and_then(|r| r.task)
        .expect("the run's record");
    let failure = ended(&mut conn, &h)
        .await
        .last_failure
        .expect("a failed run");
    assert_eq!(failure.task.as_deref(), Some(run.as_str()));
    assert!(
        failure.reason.contains("start prompt was not typed"),
        "{}",
        failure.reason
    );
    let record = h.record(&run);
    assert_eq!(record["status"], "dropped", "{record}");
    assert_eq!(record["loop"]["state"], "stopped", "{record}");
    assert_eq!(record["loop"]["reason"], "not-sent", "{record}");
    let tagged = server
        .tmux(&["list-sessions", "-F", "#{@muxa_task}"])
        .lines()
        .any(|t| t == run);
    assert!(tagged, "a failed run keeps its session");

    h.steer("answered", &json!(true));
    // Longer than the loop's settle time and a few daemon ticks.
    let deadline = Instant::now() + Duration::from_secs(15);
    while Instant::now() < deadline {
        assert!(h.turns().is_empty(), "nothing is typed: {:?}", h.turns());
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    assert_eq!(h.record(&run)["loop"]["reason"], "not-sent");
    daemon.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn without_a_daemon_a_bridge_lists_routines_but_runs_none() {
    let server = Server::start();
    let h = Host::new("bridge");
    let mut flags = h.flags();
    flags.extend([
        "--daemon-dir".into(),
        h.base.join("no-daemon").display().to_string(),
    ]);
    let flags: Vec<&str> = flags.iter().map(String::as_str).collect();
    let (mut bridge, hello) = Bridge::start(&server, &flags).await;
    let Message::Hello { capabilities, .. } = &hello else {
        panic!("{hello:?}")
    };
    // The bridge serves ask sessions (ADR 0031) too, so it advertises the
    // highest level, 5 (ADR 0039). Its daemon answers the item requests a bridge forwards.
    assert_eq!(work_level(capabilities), muxa_protocol::WORK_CLOSE_ASK);
    assert!(matches!(
        bridge.request(json!({"type": "list-routines"})).await,
        Message::Result { error: None, .. }
    ));
    let mut listed = Vec::new();
    while let Some(msg) = bridge.event(Duration::from_millis(300)).await {
        if let Message::RoutineList { entries, .. } = msg {
            listed = entries;
        }
    }
    let names: Vec<_> = listed.iter().map(|r| r.name.as_str()).collect();
    assert_eq!(names, ["Brief", "Off", "Codex"]);
    let off = listed.iter().find(|r| r.name == "Off").unwrap();
    assert_eq!((off.enabled, off.next_due), (false, None));
    let refused = bridge
        .request_within(
            json!({"type": "run-routine", "workspace": "w", "routine": "Brief"}),
            Duration::from_secs(10),
        )
        .await;
    match refused {
        Message::Result { error: Some(e), .. } => {
            assert_eq!(e.code, ErrorCode::Unsupported);
            assert!(e.message.contains("muxa daemon"), "{}", e.message);
        }
        other => panic!("{other:?}"),
    }
    assert!(matches!(
        bridge.request(json!({"type": "list-inbox"})).await,
        Message::Result { error: None, .. }
    ));
    let unknown = bridge
        .request(json!({"type": "update-inbox", "entry": "e-00000000", "action": "dismiss"}))
        .await;
    assert!(
        matches!(unknown, Message::Result { error: Some(ref e), .. }
        if e.code == ErrorCode::BadRequest),
        "{unknown:?}"
    );
    assert!(!Path::new(&h.base.join("root/routines")).exists());
}

/// The daemon's tick starts a scheduled run once its local time passed, and
/// records the occurrence, so the same time does not run again.
#[tokio::test(flavor = "multi_thread")]
async fn the_daemon_starts_a_scheduled_run_at_its_time() {
    // A minute at least 30 s away: the daemon's first look comes before it.
    let at = jiff::Timestamp::now()
        .checked_add(jiff::SignedDuration::from_secs(90))
        .unwrap()
        .to_zoned(jiff::tz::TimeZone::UTC);
    let trigger = format!("{:02}:{:02}", at.hour(), at.minute());
    let h = Host::with(
        "tick",
        &format!(
            "[[routine]]\nname = \"Tick\"\ninstructions = \"brief.md\"\n\
             at = \"{trigger}\"\ntimezone = \"UTC\"\nenabled = true\n"
        ),
    );
    h.steer("first.json", &output(json!([briefing()])));
    let server = Server::start();
    let flags = h.flags();
    let flags: Vec<&str> = flags.iter().map(String::as_str).collect();
    let mut daemon = Daemon::start(&server, &flags).await;
    let mut conn = daemon.connect().await;
    let tick = |list: Vec<RoutineInfo>| list.into_iter().find(|r| r.name == "Tick").unwrap();
    let first = tick(routines(&mut conn).await);
    assert!(first.running.is_none() && first.last_success.is_none());
    let due = first.next_due.expect("a next due time");
    assert_eq!(due % 60, 0);
    let deadline = Instant::now() + Duration::from_secs(240);
    let done = loop {
        let r = tick(routines(&mut conn).await);
        if r.last_success.is_some() && r.running.is_none() {
            break r;
        }
        assert!(
            Instant::now() < deadline,
            "no scheduled run: {r:?}; turns: {:?}",
            h.turns()
        );
        tokio::time::sleep(Duration::from_secs(1)).await;
    };
    let success = done.last_success.unwrap();
    assert!(
        success.occurrence.ends_with(&format!("T{trigger}")),
        "{success:?}"
    );
    assert!(success.at >= due);
    assert!(done.next_due.unwrap() >= due + 86_400 - 3600);
    tokio::time::sleep(Duration::from_secs(5)).await;
    assert_eq!(
        tick(routines(&mut conn).await).last_success.unwrap().task,
        success.task
    );
    assert_eq!(inbox(&mut conn).await.len(), 1);
    daemon.shutdown().await;
}
