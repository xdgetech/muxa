//! Captures (ADR 0046 slice A2), end to end: a bridge or a daemon on a
//! throwaway tmux server saves the user's input, returns the same Capture
//! for a resend, lists it, and starts nothing.
#![cfg(feature = "tmux-integration")]

mod common;

use std::path::PathBuf;

use common::daemon::Daemon;
use common::{Bridge, Server};
use muxa_protocol::{Capture, CaptureState, ErrorCode, Message, TaskRecord, WorkItem, WorkState};
use serde_json::{Value, json};

/// Must never reach a log.
const SECRET: &str = "SECRET-c4pture-7f3a";

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
    fn new(name: &str) -> Host {
        let server = Server::start();
        let base =
            std::env::temp_dir().join(format!("muxa-assistant-it-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(base.join("state")).unwrap();
        std::fs::create_dir_all(base.join("home")).unwrap();
        let root = base.join("home/work");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(
            base.join("workspaces.toml"),
            format!(
                "[[workspace]]\nname = \"work\"\nroot = \"{}\"\n",
                root.display()
            ),
        )
        .unwrap();
        Host { server, base }
    }

    fn args(&self) -> Vec<String> {
        vec![
            "--state-dir".into(),
            self.base.join("state").display().to_string(),
            "--workspaces".into(),
            self.base.join("workspaces.toml").display().to_string(),
        ]
    }

    fn sessions(&self) -> String {
        self.server.tmux(&["list-sessions", "-F", "#{session_id}"])
    }
}

fn capture_req(client_id: &str, text: &str) -> Value {
    json!({"type": "capture", "clientId": client_id, "text": text, "source": "app"})
}

fn saved(messages: &[Message]) -> Capture {
    messages
        .iter()
        .find_map(|m| match m {
            Message::CaptureSaved { entry, .. } => Some(entry.clone()),
            _ => None,
        })
        .unwrap_or_else(|| panic!("no capture-saved: {messages:?}"))
}

fn ok(messages: &[Message]) -> bool {
    matches!(messages.last(), Some(Message::Result { error: None, .. }))
}

/// Everything a bridge sends for one request, up to its result.
async fn call(bridge: &mut Bridge, req: Value) -> Vec<Message> {
    let result = bridge.request(req).await;
    let mut out = Vec::new();
    while let Some(m) = bridge.event(std::time::Duration::from_millis(100)).await {
        out.push(m);
    }
    out.push(result);
    out
}

fn listed_captures(messages: &[Message]) -> Vec<Capture> {
    messages
        .iter()
        .find_map(|m| match m {
            Message::InboxList { captures, .. } => Some(captures.clone()),
            _ => None,
        })
        .expect("an inbox-list")
}

#[tokio::test]
async fn a_capture_is_saved_once_and_starts_nothing() {
    let host = Host::new("bridge");
    let (sessions, layout) = (host.sessions(), host.server.layout());
    let no_daemon = host.base.join("no-daemon").display().to_string();
    let mut args = host.args();
    args.extend(["--daemon-dir".into(), no_daemon]);
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    let text = format!("Onboarding could be shorter {SECRET}");

    let (mut bridge, _) = Bridge::start(&host.server, &args).await;
    let first = call(&mut bridge, capture_req("client-1", &text)).await;
    assert!(ok(&first), "{first:?}");
    let entry = saved(&first);
    assert!(entry.id.starts_with("c-"));
    assert_eq!(
        (entry.text.as_str(), entry.state),
        (text.as_str(), CaptureState::Received)
    );
    assert_eq!(entry.workspace, None);
    drop(bridge);

    // A client that never saw the answer sends the capture again.
    let (mut bridge, _) = Bridge::start(&host.server, &args).await;
    let again = call(&mut bridge, capture_req("client-1", &text)).await;
    assert!(ok(&again), "{again:?}");
    assert_eq!(saved(&again), entry);
    let listed = call(&mut bridge, json!({"type": "list-inbox"})).await;
    assert_eq!(listed_captures(&listed), vec![entry]);

    // Saving starts no agent and changes no layout (invariant 2).
    assert_eq!(host.sessions(), sessions);
    assert_eq!(host.server.layout(), layout);
}

#[tokio::test]
async fn a_bad_capture_is_refused_and_assist_keeps_the_input() {
    let host = Host::new("refused");
    let no_daemon = host.base.join("no-daemon").display().to_string();
    let mut args = host.args();
    args.extend(["--daemon-dir".into(), no_daemon]);
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    let (mut bridge, _) = Bridge::start(&host.server, &args).await;

    let mut unknown = capture_req("client-2", "x");
    unknown["workspace"] = json!("nowhere");
    let refused = call(&mut bridge, unknown).await;
    assert!(matches!(
        refused.last(),
        Some(Message::Result { error: Some(e), .. }) if e.code == ErrorCode::BadRequest
    ));

    let mut assist = capture_req("client-3", "Compare the vendors");
    assist["workspace"] = json!("work");
    assist["assist"] = json!({"scope": "compare"});
    let answer = call(&mut bridge, assist).await;
    let entry = saved(&answer);
    assert_eq!(entry.workspace.as_deref(), Some("work"));
    assert!(matches!(
        answer.last(),
        Some(Message::Result { error: Some(e), .. }) if e.code == ErrorCode::Unsupported
    ));
    let listed = call(&mut bridge, json!({"type": "list-inbox"})).await;
    assert_eq!(listed_captures(&listed), vec![entry]);
}

#[tokio::test]
async fn the_daemon_saves_a_capture_and_logs_no_text() {
    let host = Host::new("daemon");
    let args = host.args();
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    let daemon = Daemon::start(&host.server, &args).await;
    let mut conn = daemon.connect().await;
    let text = format!("Renew the visa {SECRET}");
    let first = conn.request(capture_req("client-4", &text)).await;
    assert!(ok(&first), "{first:?}");
    let again = conn.request(capture_req("client-4", &text)).await;
    assert_eq!(saved(&again), saved(&first));
    let listed = conn.request(json!({"type": "list-inbox"})).await;
    assert_eq!(listed_captures(&listed).len(), 1);
    let log = daemon.log();
    assert!(log.contains("capture saved"), "{log}");
    assert!(!log.contains(SECRET), "the daemon logged the text");
}

fn work_saved(messages: &[Message]) -> WorkItem {
    messages
        .iter()
        .find_map(|m| match m {
            Message::WorkSaved { entry, .. } => Some(entry.clone()),
            Message::CaptureSaved { work: Some(w), .. } => Some((**w).clone()),
            _ => None,
        })
        .unwrap_or_else(|| panic!("no work item: {messages:?}"))
}

fn task_saved(messages: &[Message]) -> TaskRecord {
    messages
        .iter()
        .find_map(|m| match m {
            Message::TaskSaved { record, .. } => Some(record.clone()),
            _ => None,
        })
        .unwrap_or_else(|| panic!("no task-saved: {messages:?}"))
}

#[tokio::test]
async fn a_work_item_is_sorted_changed_and_starts_one_task_per_rev() {
    let host = Host::new("work");
    let no_daemon = host.base.join("no-daemon").display().to_string();
    let mut args = host.args();
    args.extend([
        "--daemon-dir".into(),
        no_daemon,
        "--task-launcher".into(),
        "builtin".into(),
        "--task-command".into(),
        "cat".into(),
    ]);
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    let (mut bridge, _) = Bridge::start(&host.server, &args).await;

    let c = saved(
        &call(
            &mut bridge,
            capture_req("client-5", "Shorten onboarding\nmore"),
        )
        .await,
    );
    let sorted = call(
        &mut bridge,
        json!({"type": "sort-capture", "capture": c.id, "rev": c.rev, "action": "work", "workspace": "work"}),
    )
    .await;
    assert!(ok(&sorted), "{sorted:?}");
    let item = work_saved(&sorted);
    assert_eq!(item.title, "Shorten onboarding");

    let stale = call(
        &mut bridge,
        json!({"type": "update-work", "work": item.id, "rev": item.rev + 1, "action": "later"}),
    )
    .await;
    assert!(matches!(
        stale.last(),
        Some(Message::Result { error: Some(e), .. }) if e.code == ErrorCode::Conflict
    ));

    let start = json!({"type": "create-task", "workspace": "work", "title": item.title,
        "goal": "Shorten the onboarding flow.", "work": item.id, "workRev": item.rev});
    let first = call(&mut bridge, start.clone()).await;
    assert!(ok(&first), "{first:?}");
    let task = task_saved(&first);
    assert_eq!(task.work.as_deref(), Some(item.id.as_str()));
    // The client never saw the answer and starts it again: the same Task.
    let again = call(&mut bridge, start).await;
    assert!(ok(&again), "{again:?}");
    assert_eq!(task_saved(&again).id, task.id);

    let listed = call(&mut bridge, json!({"type": "list-inbox"})).await;
    let work = listed
        .iter()
        .find_map(|m| match m {
            Message::InboxList { work, .. } => Some(work.clone()),
            _ => None,
        })
        .expect("an inbox-list");
    assert_eq!(work.len(), 1);
    assert_eq!(work[0].tasks, vec![task.id.clone()]);
    assert_eq!(work[0].state, WorkState::InProgress);
    assert_eq!(work[0].rev, item.rev + 1);

    // A bridge without the daemon answers get-work itself.
    let got = call(&mut bridge, json!({"type": "get-work", "work": item.id})).await;
    assert!(ok(&got), "{got:?}");
    assert!(
        got.iter().any(|m| matches!(m, Message::WorkDetail { entry, captures, .. }
            if entry.state == WorkState::InProgress && captures[0].text == "Shorten onboarding\nmore")),
        "{got:?}"
    );
}

#[tokio::test]
async fn get_work_returns_a_sorted_captures_text_and_logs_none() {
    let host = Host::new("get-work");
    let args = host.args();
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    let daemon = Daemon::start(&host.server, &args).await;
    let mut conn = daemon.connect().await;
    let text = format!("Shorten onboarding\nsteps 3 to 5 {SECRET}");
    let c = saved(&conn.request(capture_req("client-6", &text)).await);
    let sorted = conn
        .request(
            json!({"type": "sort-capture", "capture": c.id, "rev": c.rev,
            "action": "work", "workspace": "work"}),
        )
        .await;
    assert!(ok(&sorted), "{sorted:?}");
    let item = work_saved(&sorted);
    // inbox-list no longer has it: it is sorted.
    let listed = conn.request(json!({"type": "list-inbox"})).await;
    assert!(listed_captures(&listed).is_empty());

    let got = conn
        .request(json!({"type": "get-work", "work": item.id}))
        .await;
    assert!(ok(&got), "{got:?}");
    let Some(Message::WorkDetail {
        entry, captures, ..
    }) = got.iter().find(|m| matches!(m, Message::WorkDetail { .. }))
    else {
        panic!("no work-detail: {got:?}");
    };
    assert_eq!(entry.id, item.id);
    assert_eq!(captures.len(), 1);
    assert_eq!(
        (captures[0].text.as_str(), captures[0].state),
        (text.as_str(), CaptureState::Sorted)
    );

    let unknown = conn
        .request(json!({"type": "get-work", "work": "w-0000ffff"}))
        .await;
    assert!(matches!(
        unknown.last(),
        Some(Message::Result { error: Some(e), .. })
            if e.code == ErrorCode::BadRequest && e.message == "no Work item w-0000ffff"
    ));
    let log = daemon.log();
    assert!(!log.contains(SECRET), "the daemon logged the text");
}

fn error_code(messages: &[Message]) -> Option<ErrorCode> {
    match messages.last() {
        Some(Message::Result { error: Some(e), .. }) => Some(e.code),
        _ => None,
    }
}

/// ADR 0047 slice B6, at the wire.
#[tokio::test]
async fn a_pinned_item_is_listed_first_and_a_due_is_kept() {
    let host = Host::new("pins");
    let mut args = host.args();
    args.extend([
        "--daemon-dir".into(),
        host.base.join("no-daemon").display().to_string(),
    ]);
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    let (mut bridge, _) = Bridge::start(&host.server, &args).await;
    let mut items = Vec::new();
    for (client, text) in [("client-p1", "First"), ("client-p2", "Second")] {
        let c = saved(&call(&mut bridge, capture_req(client, text)).await);
        let sorted = call(
            &mut bridge,
            json!({"type": "sort-capture", "capture": c.id, "rev": c.rev, "action": "work"}),
        )
        .await;
        items.push(work_saved(&sorted));
    }
    let first = &items[0];
    let pin = json!({"type": "update-work", "work": first.id, "rev": first.rev, "action": "pin"});
    let pinned = work_saved(&call(&mut bridge, pin).await);
    assert!(pinned.pinned_at.is_some());
    let stale =
        json!({"type": "update-work", "work": first.id, "rev": first.rev, "action": "unpin"});
    assert_eq!(
        error_code(&call(&mut bridge, stale).await),
        Some(ErrorCode::Conflict)
    );

    let due = json!({"type": "update-work", "work": first.id, "rev": pinned.rev, "action": "edit",
                     "due": {"at": 1762930800u64, "timezone": "Asia/Tokyo"}});
    let dated = work_saved(&call(&mut bridge, due).await);
    assert_eq!(dated.due.as_ref().map(|d| d.at), Some(1_762_930_800));
    let bad = json!({"type": "update-work", "work": first.id, "rev": dated.rev, "action": "edit",
                     "due": {"at": 1, "timezone": "Mars/Olympus"}});
    assert_eq!(
        error_code(&call(&mut bridge, bad).await),
        Some(ErrorCode::BadRequest)
    );

    let listed = call(&mut bridge, json!({"type": "list-inbox"})).await;
    let ids: Vec<String> = listed
        .iter()
        .find_map(|m| match m {
            Message::InboxList { work, .. } => Some(work.iter().map(|w| w.id.clone()).collect()),
            _ => None,
        })
        .expect("an inbox-list");
    assert_eq!(ids, [items[0].id.clone(), items[1].id.clone()]);

    // apply-notes reaches the host's assistant code, not the B0 refusal.
    let apply = json!({"type": "apply-notes", "source": "q-00000000", "version": 1,
                       "base": "0".repeat(64)});
    assert_eq!(
        error_code(&call(&mut bridge, apply).await),
        Some(ErrorCode::BadRequest)
    );
}
