//! Assistant requests (ADR 0048 slice A8) end to end: a real daemon on a
//! throwaway tmux server runs requests with the fake agent of
//! `data/assistant-agent.py`. One request per Work item rev and one run per
//! version, also after a crash; the result check and its fail prompt; a new
//! version, a cancel, a timeout, budgets and agent refusals; no content in
//! the daemon's log; the windows that were there do not change.
#![cfg(feature = "tmux-integration")]

mod common;

use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use common::daemon::{Conn, Daemon};
use common::{Bridge, Server};
use muxa_protocol::{AssistantRequest, Delivery, ErrorCode, Message, WorkItem};
use serde_json::{Value, json};

const AGENT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/data/assistant-agent.py");
/// Content that must never reach the daemon's log (ADR 0048 §11).
const MARKS: [&str; 3] = ["capture-mark-5e1a", "purpose-mark-0c7d", "result-mark-b93f"];
const FAIL_PROMPT: &str = "did not pass the check";
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
    /// Workspace `w` delegates with `extra`; `cx` delegates to Codex;
    /// `plain` takes no requests.
    fn new(name: &str, extra: &str) -> Host {
        let base = std::env::temp_dir().join(format!("muxa-arun-it-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        for d in ["w", "cx", "plain", "state", "steer", "bin"] {
            std::fs::create_dir_all(base.join(d)).unwrap();
        }
        let base = base.canonicalize().unwrap();
        // The program's name is how a delegate's adapter is known (ADR 0048 §7).
        let claude = base.join("bin/claude");
        std::fs::write(&claude, format!("#!/bin/sh\nexec python3 {AGENT} \"$@\"\n")).unwrap();
        std::fs::set_permissions(&claude, std::fs::Permissions::from_mode(0o755)).unwrap();
        std::fs::write(
            base.join("workspaces.toml"),
            format!(
                "[assistant]\nenabled = true\n\
                 [[workspace]]\nname = \"w\"\nroot = \"{w}\"\n\
                 [workspace.assistant]\ndelegate = true\n{extra}\n\
                 [[workspace]]\nname = \"cx\"\nroot = \"{cx}\"\n\
                 [workspace.agent]\ncommand = \"codex\"\n\
                 [workspace.assistant]\ndelegate = true\n\
                 [[workspace]]\nname = \"plain\"\nroot = \"{plain}\"\n",
                w = base.join("w").display(),
                cx = base.join("cx").display(),
                plain = base.join("plain").display(),
            ),
        )
        .unwrap();
        Host {
            server: Server::start(),
            base,
        }
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
            format!(
                "{} {}",
                self.base.join("bin/claude").display(),
                self.base.join("steer").display()
            ),
        ]
    }

    async fn daemon(&self) -> Daemon {
        let flags = self.flags();
        let flags: Vec<&str> = flags.iter().map(String::as_str).collect();
        Daemon::start(&self.server, &flags).await
    }

    fn steer(&self, name: &str, value: &Value) {
        std::fs::write(self.base.join("steer").join(name), value.to_string()).unwrap();
    }

    fn unsteer(&self, name: &str) {
        let _ = std::fs::remove_file(self.base.join("steer").join(name));
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

    fn edit(&self, path: PathBuf, change: impl FnOnce(&mut Value)) {
        let mut v: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        change(&mut v);
        std::fs::write(path, v.to_string()).unwrap();
    }

    /// Records of kind `assistant-run`.
    fn runs(&self) -> usize {
        std::fs::read_dir(self.base.join("state"))
            .unwrap()
            .filter_map(|e| std::fs::read_to_string(e.unwrap().path()).ok())
            .filter_map(|t| serde_json::from_str::<Value>(&t).ok())
            .filter(|v| v["kind"] == "assistant-run")
            .count()
    }

    fn tagged(&self, task: &str) -> bool {
        self.server
            .tmux(&["list-sessions", "-F", "#{@muxa_task}"])
            .lines()
            .any(|t| t == task)
    }

    fn dir(&self, task: &str) -> PathBuf {
        PathBuf::from(self.record(task)["dir"].as_str().unwrap())
    }
}

fn result(summary: &str, sources: Value) -> Value {
    json!({"version": 1, "request": "@", "requestVersion": 0, "summary": summary,
           "findings": [{"text": "Plan A costs less.", "sources": sources}]})
}

fn good() -> Value {
    result(
        &format!("Two plans fit. {}", MARKS[2]),
        json!([{"url": "https://example.com/pricing"}]),
    )
}

fn invalid() -> Value {
    result("No sources.", json!([]))
}

fn error_of(answer: &[Message]) -> (ErrorCode, String) {
    match answer.last() {
        Some(Message::Result { error: Some(e), .. }) => (e.code, e.message.clone()),
        other => panic!("expected an error: {other:?}"),
    }
}

/// A Work item in `workspace`, made from a Capture.
async fn work_item(conn: &mut Conn, client: &str, workspace: &str) -> WorkItem {
    let text = format!("Pick a host. {}", MARKS[0]);
    let saved = conn
        .request(json!({"type": "capture", "clientId": client, "text": text,
                        "workspace": workspace}))
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

async fn ask_raw(conn: &mut Conn, work: &WorkItem, workspace: Option<&str>) -> Vec<Message> {
    let mut req = json!({"type": "request-assistant", "work": work.id, "rev": work.rev,
                         "scope": "research", "purpose": format!("Cheapest. {}", MARKS[1])});
    if let Some(ws) = workspace {
        req["workspace"] = json!(ws);
    }
    conn.request(req).await
}

async fn ask(conn: &mut Conn, work: &WorkItem) -> (WorkItem, AssistantRequest) {
    let answer = ask_raw(conn, work, None).await;
    match answer.as_slice() {
        [
            Message::WorkSaved {
                entry,
                request: Some(q),
                ..
            },
            Message::Result { error: None, .. },
        ] => (entry.clone(), (**q).clone()),
        other => panic!("request-assistant: {other:?}"),
    }
}

async fn request(conn: &mut Conn, id: &str) -> AssistantRequest {
    let listed = conn.request(json!({"type": "list-inbox"})).await;
    let Some(Message::InboxList { requests, .. }) = listed.first() else {
        panic!("{listed:?}")
    };
    requests
        .iter()
        .find(|q| q.id == id)
        .cloned()
        .unwrap_or_else(|| panic!("no request {id}: {requests:?}"))
}

/// Waits until `id` is `want`, and returns it.
async fn until(conn: &mut Conn, h: &Host, id: &str, want: Delivery) -> AssistantRequest {
    let deadline = Instant::now() + RUN_WAIT;
    loop {
        let q = request(conn, id).await;
        if q.delivery == want {
            return q;
        }
        assert!(
            Instant::now() < deadline,
            "not {want:?}: {q:?}; turns: {:?}",
            h.turns()
        );
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
}

async fn eventually(what: &str, mut cond: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(30);
    while !cond() {
        assert!(Instant::now() < deadline, "{what}");
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_request_runs_once_its_result_is_stored_and_its_session_closes() {
    let h = Host::new("main", "");
    h.steer("first.json", &invalid());
    h.steer("fixed.json", &good());
    let mut daemon = h.daemon().await;
    let mut conn = daemon.connect().await;
    let before = h.server.layout();

    // Refusals come before any run folder (ADR 0048 §7).
    let plain = work_item(&mut conn, "c-plain", "plain").await;
    let (code, message) = error_of(&ask_raw(&mut conn, &plain, None).await);
    assert_eq!(code, ErrorCode::Unsupported);
    assert!(message.contains("takes no assistant requests"), "{message}");
    let codex = work_item(&mut conn, "c-codex", "cx").await;
    let (code, message) = error_of(&ask_raw(&mut conn, &codex, None).await);
    assert_eq!(code, ErrorCode::Unsupported);
    assert!(message.contains("Codex"), "{message}");
    assert!(!h.base.join("cx/assistant").exists());

    let item = work_item(&mut conn, "c-main", "w").await;
    let mut stale = item.clone();
    stale.rev += 5;
    assert_eq!(
        error_of(&ask_raw(&mut conn, &stale, None).await).0,
        ErrorCode::Conflict
    );
    let (asked, q) = ask(&mut conn, &item).await;
    assert_eq!((q.version, q.workspace.as_str()), (1, "w"));
    assert_eq!(asked.requests, vec![q.id.clone()]);
    let run = q.run.clone().expect("a run");
    // The same rev again: the same request and run, nothing new.
    let (_, again) = ask(&mut conn, &item).await;
    assert_eq!(
        (again.id.as_str(), again.run.as_deref()),
        (q.id.as_str(), Some(run.as_str()))
    );
    assert_eq!(h.runs(), 1);

    let done = until(&mut conn, &h, &q.id, Delivery::Resulted).await;
    assert_eq!(done.results.len(), 1);
    assert!(done.results[0].summary.contains(MARKS[2]));
    let turns = h.turns();
    assert_eq!(
        turns.iter().filter(|t| t.contains(FAIL_PROMPT)).count(),
        1,
        "{turns:?}"
    );
    let dir = h.dir(&run);
    assert!(dir.starts_with(h.base.join("w/assistant")));
    let log = std::fs::read_to_string(dir.join(".muxa/checks/1.log")).unwrap();
    assert!(log.contains("findings[0].sources"), "{log}");
    let task_md = std::fs::read_to_string(dir.join(".muxa/TASK.md")).unwrap();
    assert!(task_md.contains(MARKS[0]) && task_md.contains(MARKS[1]));
    assert!(task_md.contains(&format!("\nmuxa-request: {} v1\n", q.id)));
    // ADR 0037: the published run's session closes; its record is done.
    eventually("the run's session stays", || !h.tagged(&run)).await;
    assert_eq!(h.record(&run)["status"], "done");
    assert_eq!(h.record(&run)["kind"], "assistant-run");
    assert!(dir.is_dir(), "the published run keeps its folder for now");
    let listed = conn.request(json!({"type": "list-tasks"})).await;
    assert!(
        matches!(listed.first(), Some(Message::TaskList { entries, .. }) if entries.is_empty()),
        "a run is not a Task: {listed:?}"
    );
    let item = match conn.request(json!({"type": "list-inbox"})).await.first() {
        Some(Message::InboxList { work, .. }) => work.iter().find(|w| w.id == item.id).cloned(),
        other => panic!("{other:?}"),
    };
    assert_eq!(item.unwrap().state, muxa_protocol::WorkState::Open);

    let after = h.server.layout();
    for line in before.lines() {
        assert!(after.lines().any(|l| l == line), "{line} changed:\n{after}");
    }
    let log = daemon.log();
    for mark in MARKS {
        assert!(!log.contains(mark), "the daemon log holds {mark}:\n{log}");
    }
    daemon.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_new_version_replaces_a_running_one_and_its_publish_clears_the_older_folder() {
    let h = Host::new("versions", "max_runs_per_day = 3");
    h.steer("stall", &json!(true));
    h.steer("first.json", &good());
    let mut daemon = h.daemon().await;
    let mut conn = daemon.connect().await;
    let item = work_item(&mut conn, "c-versions", "w").await;
    let (item, v1) = ask(&mut conn, &item).await;
    let run1 = v1.run.clone().unwrap();
    until(&mut conn, &h, &v1.id, Delivery::Processing).await;

    h.unsteer("stall");
    let (item, v2) = ask(&mut conn, &item).await;
    assert_eq!((v2.id.as_str(), v2.version), (v1.id.as_str(), 2));
    let run2 = v2.run.clone().unwrap();
    assert_ne!(run2, run1);
    eventually("version 1's session stays", || !h.tagged(&run1)).await;
    assert_eq!(h.record(&run1)["status"], "dropped");
    let done = until(&mut conn, &h, &v2.id, Delivery::Resulted).await;
    assert_eq!(
        done.results.iter().map(|r| r.version).collect::<Vec<_>>(),
        [2]
    );
    eventually("version 2's session stays", || !h.tagged(&run2)).await;

    let (item, v3) = ask(&mut conn, &item).await;
    assert_eq!(v3.version, 3);
    assert!(
        v3.results[0].stale,
        "a newer version makes older results stale"
    );
    let run3 = v3.run.clone().unwrap();
    until(&mut conn, &h, &v3.id, Delivery::Resulted).await;
    let dir2 = h.dir(&run2);
    eventually("version 2's folder stays after version 3 published", || {
        !dir2.is_dir()
    })
    .await;
    assert!(h.dir(&run1).is_dir(), "a replaced run keeps its folder");
    assert!(
        h.dir(&run3).is_dir(),
        "the newest published run keeps its folder"
    );

    let (code, message) = error_of(&ask_raw(&mut conn, &item, None).await);
    assert_eq!(code, ErrorCode::Conflict);
    assert!(
        message.contains("max_runs_per_day") && message.contains("resets at"),
        "{message}"
    );
    daemon.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_cancel_closes_the_run_and_a_late_result_is_not_stored() {
    let h = Host::new("cancel", "");
    h.steer("stall", &json!(true));
    h.steer("first.json", &good());
    let mut daemon = h.daemon().await;
    let mut conn = daemon.connect().await;
    let item = work_item(&mut conn, "c-cancel", "w").await;
    let (_, q) = ask(&mut conn, &item).await;
    let run = q.run.clone().unwrap();
    until(&mut conn, &h, &q.id, Delivery::Processing).await;

    let wrong = conn
        .request(json!({"type": "cancel-request", "request": q.id, "version": 2}))
        .await;
    assert_eq!(error_of(&wrong).0, ErrorCode::Conflict);
    let cancel = json!({"type": "cancel-request", "request": q.id, "version": 1});
    let answer = conn.request(cancel.clone()).await;
    let Some(Message::WorkSaved {
        request: Some(saved),
        ..
    }) = answer.first()
    else {
        panic!("{answer:?}")
    };
    assert!(matches!(
        saved.delivery,
        Delivery::CancelRequested | Delivery::Cancelled
    ));
    until(&mut conn, &h, &q.id, Delivery::Cancelled).await;
    assert!(!h.tagged(&run));
    assert_eq!(h.record(&run)["status"], "dropped");

    // The agent's result arrives late, as a turn cut short would leave it.
    let dir = h.dir(&run);
    let mut late = good();
    late["request"] = json!(q.id);
    late["requestVersion"] = json!(1);
    std::fs::write(dir.join(".muxa/assistant/result.json"), late.to_string()).unwrap();
    h.steer("release", &json!(true));
    tokio::time::sleep(Duration::from_secs(6)).await;
    let after = request(&mut conn, &q.id).await;
    assert!(after.results.is_empty(), "{after:?}");
    assert_eq!(after.delivery, Delivery::Cancelled);
    assert!(matches!(
        conn.request(cancel).await.last(),
        Some(Message::Result { error: None, .. })
    ));
    daemon.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn invalid_results_fail_the_request_after_three_checks() {
    let h = Host::new("invalid", "");
    h.steer("first.json", &invalid());
    h.steer("fixed.json", &invalid());
    let mut daemon = h.daemon().await;
    let mut conn = daemon.connect().await;
    // `capture {assist}` makes the Work item and the request at once.
    let assist = json!({"type": "capture", "clientId": "c-assist", "workspace": "w",
                        "text": format!("Pick a host. {}", MARKS[0]),
                        "assist": {"scope": "compare", "purpose": MARKS[1]}});
    let first = conn.request(assist.clone()).await;
    let Some(Message::CaptureSaved {
        entry,
        work: Some(work),
        request: Some(q),
        ..
    }) = first.first()
    else {
        panic!("{first:?}")
    };
    assert_eq!(entry.state, muxa_protocol::CaptureState::Sorted);
    assert_eq!(work.requests, vec![q.id.clone()]);
    let resent = conn.request(assist).await;
    assert!(
        matches!(resent.first(), Some(Message::CaptureSaved { request: Some(r), .. })
        if r.id == q.id && r.run == q.run)
    );
    let run = q.run.clone().unwrap();

    until(&mut conn, &h, &q.id, Delivery::Failed).await;
    let turns = h.turns();
    assert_eq!(
        turns.iter().filter(|t| t.contains(FAIL_PROMPT)).count(),
        2,
        "{turns:?}"
    );
    let deadline = Instant::now() + Duration::from_secs(20);
    while h.record(&run)["status"] != "dropped" {
        assert!(Instant::now() < deadline, "{}", h.record(&run));
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
    assert!(
        h.tagged(&run),
        "a failed run keeps its session for inspection"
    );
    assert_eq!(h.runs(), 1);
    let log = daemon.log();
    for mark in MARKS {
        assert!(!log.contains(mark), "the daemon log holds {mark}:\n{log}");
    }
    daemon.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_timed_out_run_keeps_its_agent_and_a_late_result_still_counts() {
    let h = Host::new("timeout", "run_timeout = \"1m\"");
    h.steer("stall", &json!(true));
    h.steer("first.json", &good());
    let mut daemon = h.daemon().await;
    let mut conn = daemon.connect().await;
    let item = work_item(&mut conn, "c-timeout", "w").await;
    let (_, q) = ask(&mut conn, &item).await;
    let run = q.run.clone().unwrap();
    until(&mut conn, &h, &q.id, Delivery::TimedOut).await;
    assert!(h.tagged(&run), "the agent is not stopped");
    h.steer("release", &json!(true));
    let done = until(&mut conn, &h, &q.id, Delivery::Resulted).await;
    assert_eq!(done.results.len(), 1);
    daemon.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_restart_finds_the_run_again_and_an_interrupted_send_is_never_retyped() {
    let h = Host::new("restart", "");
    h.steer("hold", &json!(true));
    h.steer("first.json", &good());
    let flags = h.flags();
    let flags: Vec<&str> = flags.iter().map(String::as_str).collect();
    let mut daemon = h.daemon().await;
    let mut conn = daemon.connect().await;
    let item = work_item(&mut conn, "c-restart", "w").await;
    let (_, q) = ask(&mut conn, &item).await;
    let run = q.run.clone().unwrap();
    assert_eq!(request(&mut conn, &q.id).await.delivery, Delivery::Pending);
    daemon.crash().await;

    // As a crash between the create and its link leaves it: a run the
    // request does not name, and its claim. The send was cut short too.
    h.edit(
        h.base
            .join(format!("state/assistant/requests/{}.json", q.id)),
        |v| {
            v.as_object_mut().unwrap().remove("run");
            v["creating"] = json!([1, 1]);
        },
    );
    h.edit(h.base.join(format!("state/{run}.json")), |v| {
        v["restartOperation"] = json!({"version": 1, "id": "delivery-0000000000000001",
            "phase": "delivering", "sourceRun": 1, "reservedRun": 1, "pane": 0,
            "sourcePid": 1, "worker": null, "since": 1});
    });
    daemon.start_again(&h.server, &flags).await;
    let mut conn = daemon.connect().await;
    let (_, again) = ask(&mut conn, &item).await;
    assert_eq!(
        (again.id.as_str(), again.run.as_deref()),
        (q.id.as_str(), Some(run.as_str()))
    );
    assert_eq!(h.runs(), 1);

    // The agent leaves its dialog; nothing types the prompt again.
    h.steer("answered", &json!(true));
    tokio::time::sleep(Duration::from_secs(10)).await;
    assert!(h.turns().is_empty(), "retyped: {:?}", h.turns());
    assert_eq!(
        request(&mut conn, &q.id).await.delivery,
        Delivery::Unconfirmed
    );
    daemon.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn without_a_daemon_automatic_processing_is_off() {
    let h = Host::new("bridge", "");
    let mut flags = h.flags();
    flags.extend([
        "--daemon-dir".into(),
        h.base.join("no-daemon").display().to_string(),
    ]);
    let flags: Vec<&str> = flags.iter().map(String::as_str).collect();
    let (mut bridge, _) = Bridge::start(&h.server, &flags).await;
    for req in [
        json!({"type": "request-assistant", "work": "w-00000000", "rev": 1, "scope": "draft"}),
        json!({"type": "cancel-request", "request": "q-00000000", "version": 1}),
    ] {
        match bridge.request_within(req, Duration::from_secs(10)).await {
            Message::Result { error: Some(e), .. } => {
                assert_eq!(e.code, ErrorCode::Unsupported);
                assert!(
                    e.message.contains("Automatic processing is off"),
                    "{}",
                    e.message
                );
            }
            other => panic!("{other:?}"),
        }
    }
    assert!(!h.base.join("w/assistant").exists());
}

/// Notes text that must never reach the daemon's log (ADR 0049 §9).
const NOTES_MARK: &str = "notes-mark-3b6f";

fn notes_path(h: &Host) -> PathBuf {
    h.base.join("w/.muxa/WORKSPACE.md")
}

fn with_notes(text: &str) -> Value {
    let mut v = good();
    v["notes"] = json!(text);
    v
}

fn sha256(bytes: &[u8]) -> String {
    use sha2::Digest;
    sha2::Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

#[tokio::test(flavor = "multi_thread")]
async fn notes_are_read_at_the_start_proposed_and_applied_only_on_request() {
    let h = Host::new("notes", "");
    let new = format!("# Hosting\n\n- {NOTES_MARK} new\n");
    h.steer("first.json", &with_notes(&new));
    let mut daemon = h.daemon().await;
    let mut conn = daemon.connect().await;
    let mut item = work_item(&mut conn, "c-notes", "w").await;

    // An oversized file or a symlink fails the start before any run exists.
    std::fs::create_dir_all(h.base.join("w/.muxa/assistant")).unwrap();
    std::fs::write(
        notes_path(&h),
        format!("{NOTES_MARK}{}", "x".repeat(16 << 10)),
    )
    .unwrap();
    let elsewhere = h.base.join("elsewhere.md");
    std::fs::write(&elsewhere, NOTES_MARK).unwrap();
    for (needle, last) in [("16399 bytes", false), ("not a regular file", true)] {
        let answer = ask_raw(&mut conn, &item, None).await;
        let (code, message) = error_of(&answer);
        assert_eq!(code, ErrorCode::BadRequest);
        assert!(
            message.contains("WORKSPACE.md") && message.contains(needle),
            "{message}"
        );
        assert!(!message.contains(NOTES_MARK), "{message}");
        match answer.first() {
            Some(Message::WorkSaved {
                entry,
                request: Some(q),
                ..
            }) => {
                assert_eq!(q.delivery, Delivery::Failed);
                item = entry.clone();
            }
            other => panic!("{other:?}"),
        }
        if !last {
            std::fs::remove_file(notes_path(&h)).unwrap();
            std::os::unix::fs::symlink(&elsewhere, notes_path(&h)).unwrap();
        }
    }
    assert_eq!(h.runs(), 0);
    assert!(!h.base.join("w/assistant").exists());

    std::fs::remove_file(notes_path(&h)).unwrap();
    let old = format!("# Hosting\n\n- {NOTES_MARK} old\n");
    std::fs::write(notes_path(&h), &old).unwrap();
    let (_, q) = ask(&mut conn, &item).await;
    let run = q.run.clone().expect("a run");
    let done = until(&mut conn, &h, &q.id, Delivery::Resulted).await;
    let copy = std::fs::read_to_string(h.dir(&run).join(".muxa/WORKSPACE.md")).unwrap();
    assert_eq!(copy, old);
    let task_md = std::fs::read_to_string(h.dir(&run).join(".muxa/TASK.md")).unwrap();
    assert!(task_md.contains(".muxa/WORKSPACE.md"), "{task_md}");
    let notes = done.results[0].notes.clone().expect("proposed notes");
    assert_eq!(
        (notes.text.as_str(), notes.base.clone()),
        (new.as_str(), sha256(old.as_bytes()))
    );
    assert_eq!(
        std::fs::read_to_string(notes_path(&h)).unwrap(),
        old,
        "review never writes at publish"
    );

    // The two failed starts made versions 1 and 2.
    assert_eq!(done.results[0].version, 3);
    let apply = json!({"type": "apply-notes", "source": q.id, "version": 3, "base": notes.base});
    let answer = conn.request(apply.clone()).await;
    match answer.as_slice() {
        [
            Message::WorkSaved {
                request: Some(saved),
                ..
            },
            Message::Result { error: None, .. },
        ] => {
            assert_eq!(saved.id, q.id)
        }
        other => panic!("apply-notes: {other:?}"),
    }
    assert_eq!(std::fs::read_to_string(notes_path(&h)).unwrap(), new);
    // Applied, the live file no longer hashes to the proposal's base.
    let (code, message) = error_of(&conn.request(apply).await);
    assert_eq!(code, ErrorCode::Conflict, "{message}");

    let log = daemon.log();
    for mark in MARKS.iter().chain([&NOTES_MARK]) {
        assert!(!log.contains(mark), "the daemon log holds {mark}:\n{log}");
    }
    daemon.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn with_notes_auto_a_passing_result_writes_the_notes_at_publish() {
    let h = Host::new("auto-notes", "notes = \"auto\"");
    let new = format!("# Hosting\n\n- {NOTES_MARK}\n");
    h.steer("first.json", &with_notes(&new));
    let mut daemon = h.daemon().await;
    let mut conn = daemon.connect().await;
    let item = work_item(&mut conn, "c-auto", "w").await;
    assert!(!notes_path(&h).exists());
    let (_, q) = ask(&mut conn, &item).await;
    let done = until(&mut conn, &h, &q.id, Delivery::Resulted).await;
    assert_eq!(done.results[0].notes.as_ref().unwrap().base, sha256(b""));
    eventually("the notes are not written", || {
        std::fs::read_to_string(notes_path(&h)).is_ok_and(|t| t == new)
    })
    .await;
    let log = daemon.log();
    assert!(log.contains("notes applied at publish"), "{log}");
    for mark in MARKS.iter().chain([&NOTES_MARK]) {
        assert!(!log.contains(mark), "the daemon log holds {mark}:\n{log}");
    }
    daemon.shutdown().await;
}
