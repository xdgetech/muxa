//! Captures (ADR 0048 slice A2), end to end: a bridge or a daemon on a
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

/// ADR 0049 slice B6, at the wire.
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

fn policy_saved(messages: &[Message]) -> muxa_protocol::Policy {
    messages
        .iter()
        .find_map(|m| match m {
            Message::PolicySaved { entry, .. } => Some(entry.clone()),
            _ => None,
        })
        .unwrap_or_else(|| panic!("no policy-saved: {messages:?}"))
}

fn listed_policies(messages: &[Message]) -> Vec<muxa_protocol::Policy> {
    messages
        .iter()
        .find_map(|m| match m {
            Message::InboxList { policies, .. } => Some(policies.clone()),
            _ => None,
        })
        .expect("an inbox-list")
}

fn error_of(messages: &[Message]) -> Option<ErrorCode> {
    match messages.last() {
        Some(Message::Result { error, .. }) => error.as_ref().map(|e| e.code),
        _ => None,
    }
}

/// ADR 0050 slice C1, through a bridge without a daemon.
#[tokio::test]
async fn a_policy_is_added_edited_and_deleted_with_its_rev() {
    let host = Host::new("policies");
    let no_daemon = host.base.join("no-daemon").display().to_string();
    let mut args = host.args();
    args.extend(["--daemon-dir".into(), no_daemon]);
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    let (mut bridge, _) = Bridge::start(&host.server, &args).await;
    let text = format!("Recruiting comes before sales {SECRET}");
    let added = call(
        &mut bridge,
        json!({"type": "add-policy", "text": text, "scope": "work"}),
    )
    .await;
    assert!(ok(&added), "{added:?}");
    let p = policy_saved(&added);
    assert_eq!(
        (p.rev, p.text.as_str(), p.scope.as_str(), p.source.as_str()),
        (1, text.as_str(), "work", "user")
    );
    let listed = call(&mut bridge, json!({"type": "list-inbox"})).await;
    assert_eq!(listed_policies(&listed), [p.clone()]);

    let edit = |rev: u64| {
        json!({"type": "update-policy", "policy": p.id, "rev": rev, "action": "edit",
               "text": "Recruiting first."})
    };
    let edited = call(&mut bridge, edit(1)).await;
    assert!(ok(&edited), "{edited:?}");
    assert_eq!(policy_saved(&edited).rev, 2);
    let stale = call(&mut bridge, edit(1)).await;
    assert_eq!(error_of(&stale), Some(ErrorCode::Conflict));
    let oversize = call(
        &mut bridge,
        json!({"type": "add-policy", "text": "x".repeat(501), "scope": "all"}),
    )
    .await;
    assert_eq!(error_of(&oversize), Some(ErrorCode::BadRequest));

    // A delete answers with the result only.
    let deleted = call(
        &mut bridge,
        json!({"type": "update-policy", "policy": p.id, "rev": 2, "action": "delete"}),
    )
    .await;
    assert!(ok(&deleted), "{deleted:?}");
    assert_eq!(deleted.len(), 1, "{deleted:?}");
    let listed = call(&mut bridge, json!({"type": "list-inbox"})).await;
    assert!(listed_policies(&listed).is_empty());
}

#[tokio::test]
async fn the_daemon_keeps_a_policy_and_logs_no_text() {
    let host = Host::new("policies-daemon");
    let args = host.args();
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    let daemon = Daemon::start(&host.server, &args).await;
    let mut conn = daemon.connect().await;
    let text = format!("Mornings are for the report {SECRET}");
    let added = conn
        .request(json!({"type": "add-policy", "text": text, "scope": "all"}))
        .await;
    assert!(ok(&added), "{added:?}");
    let listed = conn.request(json!({"type": "list-inbox"})).await;
    assert_eq!(listed_policies(&listed)[0].text, text);
    let log = daemon.log();
    assert!(log.contains("policy added"), "{log}");
    assert!(!log.contains(SECRET), "the daemon logged the policy's text");
}

fn plan_saved(messages: &[Message]) -> muxa_protocol::Plan {
    messages
        .iter()
        .find_map(|m| match m {
            Message::PlanSaved { entry, .. } => Some((**entry).clone()),
            _ => None,
        })
        .unwrap_or_else(|| panic!("no plan-saved: {messages:?}"))
}

/// ADR 0050 slice C2, through the daemon: each action on the current plan
/// records one correction, and a reason never reaches the log.
#[tokio::test]
async fn the_daemon_records_corrections_and_logs_no_reason() {
    let host = Host::new("corrections");
    let args = host.args();
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    let daemon = Daemon::start(&host.server, &args).await;
    let mut conn = daemon.connect().await;
    let mut items = Vec::new();
    for client in ["client-x1", "client-x2"] {
        let c = saved(
            &conn
                .request(capture_req(client, "Prepare the report"))
                .await,
        );
        let sorted = conn
            .request(
                json!({"type": "sort-capture", "capture": c.id, "rev": c.rev, "action": "work"}),
            )
            .await;
        items.push(work_saved(&sorted));
    }
    let (a, b) = (&items[0], &items[1]);
    let plan = "p-0a1b2c3d";
    let row = |w: &WorkItem| {
        json!({"work": w.id, "why": "w", "next": "n",
               "minutes": {"low": 1, "high": 2, "estimated": 1}})
    };
    let dir = host.base.join("state/assistant/plans");
    std::fs::create_dir_all(&dir).unwrap();
    let file = json!({"version": 1, "id": plan, "rev": 2, "at": 100, "inputsAt": 100,
                      "delivery": "resulted", "deliveryAt": 100, "top": [row(a), row(b)]});
    std::fs::write(dir.join(format!("{plan}.json")), file.to_string()).unwrap();

    let pin = json!({"type": "update-work", "work": a.id, "rev": a.rev, "action": "pin"});
    assert!(ok(&conn.request(pin).await));
    let reorder = json!({"type": "update-plan", "plan": plan, "rev": 2, "action": "reorder",
                         "order": [b.id, a.id]});
    let reordered = plan_saved(&conn.request(reorder).await);
    assert_eq!(
        (reordered.rev, reordered.user_order.clone()),
        (3, vec![b.id.clone(), a.id.clone()])
    );
    let reason = format!("Waiting for the vendor {SECRET}");
    let dismiss = |rev: u64| {
        json!({"type": "update-plan", "plan": plan, "rev": rev, "action": "dismiss",
               "row": a.id, "text": reason})
    };
    let dismissed = plan_saved(&conn.request(dismiss(3)).await);
    assert_eq!(dismissed.dismissed, [a.id.clone()]);
    assert_eq!(
        error_of(&conn.request(dismiss(3)).await),
        Some(ErrorCode::Conflict)
    );

    let listed = conn.request(json!({"type": "list-inbox"})).await;
    let corrections = listed
        .iter()
        .find_map(|m| match m {
            Message::InboxList { corrections, .. } => Some(corrections.clone()),
            _ => None,
        })
        .expect("an inbox-list");
    let mut kinds: Vec<String> = corrections
        .iter()
        .map(|c| format!("{:?} {} {}", c.kind, c.plan, c.in_top))
        .collect();
    kinds.sort();
    assert_eq!(
        kinds,
        [
            format!("Dismissed {plan} true"),
            format!("Pinned {plan} true"),
            format!("Reordered {plan} true")
        ]
    );
    let with_reason: Vec<_> = corrections
        .iter()
        .filter_map(|c| c.reason.as_ref())
        .collect();
    assert_eq!(with_reason, [&reason]);
    let log = daemon.log();
    assert!(log.contains("correction recorded"), "{log}");
    assert!(!log.contains(SECRET), "the daemon logged a reason");
}
