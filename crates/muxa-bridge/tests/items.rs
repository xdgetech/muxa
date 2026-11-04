//! Inbox items from agent states (ADR 0033 §2), end to end: a Task's
//! session on a throwaway tmux server, its agent's state as tags, and a
//! bridge or a daemon that makes and resolves the items in the state dir.
#![cfg(feature = "tmux-integration")]

mod common;

use std::path::{Path, PathBuf};
use std::time::Duration;

use common::daemon::Daemon;
use common::{Bridge, Server};
use muxa_protocol::Message;
use serde_json::{Value, json};

const TASK: &str = "t-00a1b2";
/// Several looks of a 2 s tick.
const WAIT: Duration = Duration::from_secs(15);

struct Host {
    server: Server,
    base: PathBuf,
    pane: String,
}

impl Drop for Host {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.base);
    }
}

impl Host {
    /// An open Task whose session's agent pane reports `state`.
    fn new(name: &str, state: &str, at: u64) -> Host {
        let server = Server::start();
        let base =
            std::env::temp_dir().join(format!("muxa-items-it-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let dir = base.join("task");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::create_dir_all(base.join("state")).unwrap();
        let record = json!({
            "version": 1, "id": TASK, "rev": 1, "workspace": "w",
            "title": "Fix the login", "goal": "Make it work.",
            "status": "open", "createdAt": 1, "updatedAt": 1,
            "dir": dir.display().to_string(),
        });
        std::fs::write(base.join(format!("state/{TASK}.json")), record.to_string()).unwrap();
        let made = server.tmux(&[
            "new-session",
            "-d",
            "-P",
            "-F",
            "#{session_id} #{pane_id}",
            "-s",
            "task",
            "-c",
            &dir.display().to_string(),
            "sleep 600",
        ]);
        let (session, pane) = made.split_once(' ').expect("session and pane");
        server.tmux(&["set-option", "-t", session, "@muxa_task", TASK]);
        server.tmux(&["set-option", "-p", "-t", pane, "@agents_role", "ai"]);
        let host = Host {
            server,
            base,
            pane: pane.to_owned(),
        };
        host.report(state, at);
        host
    }

    fn report(&self, state: &str, at: u64) {
        let pane = self.pane.as_str();
        let at = at.to_string();
        self.server
            .tmux(&["set-option", "-p", "-t", pane, "@agent_state_at", &at]);
        self.server
            .tmux(&["set-option", "-p", "-t", pane, "@agent_state", state]);
    }

    fn state_dir(&self) -> String {
        self.base.join("state").display().to_string()
    }

    fn items(&self) -> Vec<Value> {
        read_items(&self.base.join(format!("state/items/{TASK}.json")))
    }

    /// Waits until `cond` holds for the stored items.
    async fn until(&self, what: &str, cond: impl Fn(&[Value]) -> bool) -> Vec<Value> {
        let deadline = tokio::time::Instant::now() + WAIT;
        loop {
            let items = self.items();
            if cond(&items) {
                return items;
            }
            assert!(
                tokio::time::Instant::now() < deadline,
                "no {what}: {items:?}"
            );
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
    }
}

fn read_items(path: &Path) -> Vec<Value> {
    let Ok(text) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    let v: Value = serde_json::from_str(&text).expect("an item file");
    v["items"].as_array().cloned().unwrap_or_default()
}

async fn a_finished_turn_makes_one_review_and_running_resolves_it(host: &Host) {
    let items = host
        .until("review", |i| i.iter().any(|x| x["kind"] == "review"))
        .await;
    assert_eq!(items.len(), 1, "{items:?}");
    let item = &items[0];
    assert_eq!(item["state"], "open");
    assert_eq!(item["key"], "review:1000");
    assert_eq!(item["question"], "Ready for review: Fix the login");
    assert_eq!(item["task"], TASK);
    assert_eq!(item["pane"], host.pane.as_str());
    assert_eq!(item["answers"][0], "accept");

    // Later looks at the same turn make nothing new.
    tokio::time::sleep(Duration::from_secs(5)).await;
    assert_eq!(host.items().len(), 1);

    host.report("running", 1010);
    let items = host
        .until("resolve", |i| {
            i.first().is_some_and(|x| x["state"] == "resolved")
        })
        .await;
    assert_eq!(items.len(), 1);
    assert!(items[0].get("answers").is_none_or(|a| a == &json!([])));

    host.report("done", 1020);
    let items = host.until("second review", |i| i.len() == 2).await;
    assert_eq!(items[1]["key"], "review:1020");
    assert_eq!(items[1]["state"], "open");
}

#[tokio::test]
async fn a_bridge_makes_and_resolves_items() {
    let host = Host::new("bridge", "done", 1000);
    let dir = host.state_dir();
    let (_bridge, _) = Bridge::start(&host.server, &["--state-dir", &dir]).await;
    a_finished_turn_makes_one_review_and_running_resolves_it(&host).await;
}

#[tokio::test]
async fn the_daemon_makes_items_with_no_client_connected() {
    let host = Host::new("daemon", "done", 1000);
    let dir = host.state_dir();
    let before = host.server.layout();
    let daemon = Daemon::start(&host.server, &["--state-dir", &dir]).await;
    a_finished_turn_makes_one_review_and_running_resolves_it(&host).await;
    assert!(daemon.log().contains("inbox item made"), "{}", daemon.log());
    // The daemon only reads tmux: no window changed (invariant 2).
    assert_eq!(before, host.server.layout());
}

/// ADR 0033 §4a: outside git the turn's new document is attached and
/// served, and `reply` types its text into the pane as it is.
#[tokio::test]
async fn a_reply_reaches_the_pane_and_the_turns_document_is_attached() {
    let host = Host::new("reply", "running", 990);
    std::fs::write(host.base.join("task/report.md"), "# Report\n").unwrap();
    let dir = host.state_dir();
    // No daemon here: answers must not reach the host's own daemon.
    let no_daemon = host.base.join("no-daemon").display().to_string();
    let (mut bridge, _) = Bridge::start(
        &host.server,
        &["--state-dir", &dir, "--daemon-dir", &no_daemon],
    )
    .await;
    let file = host.base.join(format!("state/items/{TASK}.json"));
    let deadline = tokio::time::Instant::now() + WAIT;
    let started = || {
        std::fs::read_to_string(&file)
            .ok()
            .and_then(|t| serde_json::from_str::<Value>(&t).ok())
            .is_some_and(|v| v["turnStart"] == 990)
    };
    while !started() {
        assert!(tokio::time::Instant::now() < deadline, "no turn start");
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    host.report("done", 1000);
    let items = host.until("review", |i| !i.is_empty()).await;
    let item = &items[0];
    assert_eq!(item["attachments"]["docs"], json!(["report.md"]));
    assert_eq!(
        item["answers"],
        json!([
            "accept",
            "reply",
            "redo",
            "change-course",
            "keep-open",
            "later"
        ])
    );

    // The pane's folder is in no repo: the docs list serves the attached file.
    let result = bridge
        .request(json!({"type": "docs", "pane": host.pane}))
        .await;
    assert!(
        matches!(result, Message::Result { error: None, .. }),
        "{result:?}"
    );
    let entries = loop {
        match bridge.event(WAIT).await {
            Some(Message::DocList { entries, .. }) => break entries,
            Some(_) => continue,
            None => panic!("no doc-list"),
        }
    };
    assert!(entries.iter().any(|e| e.path == "report.md"), "{entries:?}");

    let text = "Also update the README";
    let result = bridge
        .request(
            json!({"type": "update-inbox", "entry": item["id"], "action": "reply", "text": text}),
        )
        .await;
    assert!(
        matches!(result, Message::Result { error: None, .. }),
        "{result:?}"
    );
    let deadline = tokio::time::Instant::now() + WAIT;
    while !host
        .server
        .capture(&host.pane)
        .iter()
        .any(|l| l.contains(text))
    {
        assert!(
            tokio::time::Instant::now() < deadline,
            "the text never reached the pane"
        );
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    let shown = host.server.capture(&host.pane).join("\n");
    assert!(!shown.contains("Feedback"), "no preface: {shown}");
    let items = host.items();
    assert_eq!(
        (items[0]["state"].as_str(), items[0]["answer"].as_str()),
        (Some("answered"), Some("reply"))
    );
}

#[tokio::test]
async fn waiting_makes_an_ask_that_a_newer_need_replaces() {
    let host = Host::new("ask", "waiting", 1000);
    let dir = host.state_dir();
    let (_bridge, _) = Bridge::start(&host.server, &["--state-dir", &dir]).await;
    let items = host.until("ask", |i| !i.is_empty()).await;
    assert_eq!(items[0]["kind"], "ask");
    assert_eq!(items[0]["question"], "The agent waits for you");
    assert_eq!(items[0]["answers"], json!(["dismiss", "later"]));

    host.report("error", 1010);
    let items = host.until("failure", |i| i.len() == 2).await;
    assert_eq!(items[0]["state"], "resolved");
    assert_eq!(items[1]["kind"], "failure");
    assert_eq!(items[1]["question"], "The agent stopped with an error");
}
