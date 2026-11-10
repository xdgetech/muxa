//! A Task's close-out into the workspace memory (ADR 0053 slice M2), end to
//! end through a host daemon on a throwaway tmux server. The fake agent is
//! the test: it writes "For the workspace" in the Task's `TASK.md`, as an
//! agent does while it works. Done and Drop make one `memory` item; approve
//! appends once; `auto` appends at once and falls back to the item; the
//! caps hold; the text never reaches the log; no existing pane changes.
#![cfg(feature = "tmux-integration")]

mod common;

use std::path::PathBuf;

use common::Server;
use common::memory::{Host, create, error_of, ok, saved, send, sha256};
use muxa_protocol::{ErrorCode, Message, TaskRecord};
use serde_json::{Value, json};

/// Section text that must never reach the daemon's log (ADR 0053 §8).
const SENTINEL: &str = "memory-mark-9e41";
/// A Task title is content too.
const TITLE: &str = "Decide the queue title-mark-2b7d";

fn dir_of(r: &TaskRecord) -> PathBuf {
    PathBuf::from(r.dir.as_deref().expect("a dir"))
}

/// What an agent does as it works: its section for the workspace.
fn write_section(r: &TaskRecord, text: &str) {
    let path = dir_of(r).join(".muxa/TASK.md");
    let old = std::fs::read_to_string(&path).unwrap();
    let new = old.replace(
        "## For the workspace\n\nNothing yet.\n",
        &format!("## For the workspace\n\n{text}\n"),
    );
    assert_ne!(old, new, "the section is in TASK.md");
    std::fs::write(&path, new).unwrap();
}

fn items(host: &Host, task: &str) -> Vec<Value> {
    let path = host.dir.join(format!("state/items/{task}.json"));
    let Ok(text) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    let v: Value = serde_json::from_str(&text).unwrap();
    v["items"].as_array().cloned().unwrap_or_default()
}

fn memory_items(host: &Host, task: &str) -> Vec<Value> {
    items(host, task)
        .into_iter()
        .filter(|i| i["kind"] == "memory")
        .collect()
}

async fn rev_of(bridge: &mut common::Bridge, id: &str) -> u64 {
    let (before, result) = send(bridge, json!({"type": "list-tasks"})).await;
    ok(&result);
    before
        .into_iter()
        .find_map(|m| match m {
            Message::TaskList { entries, .. } => entries.into_iter().find(|r| r.id == id),
            _ => None,
        })
        .expect("the record")
        .rev
}

async fn end(bridge: &mut common::Bridge, r: &TaskRecord, status: &str, extra: Value) -> Message {
    let mut req = json!({"type": "update-task", "task": r.id, "rev": rev_of(bridge, &r.id).await, "status": status});
    for (k, v) in extra.as_object().unwrap() {
        req[k] = v.clone();
    }
    saved(bridge, req).await.1
}

/// A started Task whose agent wrote `section`.
async fn task(bridge: &mut common::Bridge, title: &str, section: Option<&str>) -> TaskRecord {
    let (r, result) = create(bridge, json!({"title": title})).await;
    ok(&result);
    let r = r.expect("task-saved");
    if let Some(text) = section {
        write_section(&r, text);
    }
    r
}

async fn answer(
    bridge: &mut common::Bridge,
    item: &str,
    action: &str,
    hash: Option<&str>,
) -> Message {
    let mut req = json!({"type": "update-inbox", "entry": item, "action": action});
    if let Some(h) = hash {
        req["hash"] = json!(h);
    }
    send(bridge, req).await.1
}

fn hash(task: &str, title: &str, text: &str) -> String {
    format!(
        "sha256:{}",
        sha256(format!("{task}\0{title}\0{text}").as_bytes())
    )
}

/// The panes that existed before, as they are now.
fn panes(server: &Server, before: &str) -> String {
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

#[tokio::test]
async fn done_and_drop_make_one_item_and_nothing_yet_makes_none() {
    let server = Server::start();
    let before = server.layout();
    let mut host = Host::new("items", "memory = \"review\"");
    let mut bridge = host.bridge(&server, true).await;
    let text = format!("- Decided: one queue per tenant, because {SENTINEL}.");

    let done = task(&mut bridge, TITLE, Some(&text)).await;
    ok(&end(&mut bridge, &done, "done", json!({})).await);
    let found = memory_items(&host, &done.id);
    assert_eq!(found.len(), 1, "{found:?}");
    let item = &found[0];
    assert_eq!(item["state"], "open");
    assert_eq!(item["key"], format!("memory:{}", done.id));
    assert_eq!(item["task"], done.id.as_str());
    assert_eq!(item["workspace"], "lisk");
    assert_eq!(item["title"], TITLE);
    assert_eq!(
        item["question"],
        format!("{TITLE} proposes to add to the workspace memory")
    );
    assert_eq!(item["memory"]["text"], text.as_str());
    assert_eq!(item["memory"]["hash"], hash(&done.id, TITLE, &text));
    assert_eq!(item["answers"], json!(["approve", "reject", "later"]));
    // Done again makes no second item.
    ok(&end(&mut bridge, &done, "done", json!({})).await);
    assert_eq!(memory_items(&host, &done.id).len(), 1);

    let dropped = task(
        &mut bridge,
        "Try a queue",
        Some("- Queues need a dead-letter topic."),
    )
    .await;
    ok(&end(&mut bridge, &dropped, "dropped", json!({})).await);
    assert_eq!(memory_items(&host, &dropped.id).len(), 1);

    let nothing = task(&mut bridge, "Tidy up", None).await;
    ok(&end(&mut bridge, &nothing, "done", json!({})).await);
    assert!(memory_items(&host, &nothing.id).is_empty());
    assert!(
        host.read_memory().is_none(),
        "review writes nothing at the end"
    );

    let log = host.log();
    assert!(log.contains("memory item made"), "{log}");
    assert!(!log.contains(SENTINEL), "the text reached the log");
    assert!(
        !log.contains("title-mark-2b7d"),
        "the title reached the log"
    );
    assert_eq!(panes(&server, &before), before, "no existing pane changed");
}

#[tokio::test]
async fn approve_appends_once_also_after_a_crash_and_reject_writes_nothing() {
    let server = Server::start();
    let mut host = Host::new("approve", "memory = \"review\"");
    host.write_memory("# Lisk\n");
    let mut bridge = host.bridge(&server, true).await;
    let text = format!("- Staging is eu-west-1 ({SENTINEL}).");
    let r = task(&mut bridge, TITLE, Some(&text)).await;
    ok(&end(&mut bridge, &r, "done", json!({})).await);
    let item = memory_items(&host, &r.id).remove(0);
    let (id, h) = (
        item["id"].as_str().unwrap(),
        item["memory"]["hash"].as_str().unwrap(),
    );

    // No hash, or another one, writes nothing.
    for wrong in [None, Some("sha256:00")] {
        let e = error_of(&answer(&mut bridge, id, "approve", wrong).await);
        assert_eq!(e.code, ErrorCode::BadRequest, "{e:?}");
    }
    assert_eq!(host.read_memory().unwrap(), "# Lisk\n");

    ok(&answer(&mut bridge, id, "approve", Some(h)).await);
    let live = host.read_memory().unwrap();
    let heading = format!("## {TITLE} ({}, ", r.id);
    assert!(live.starts_with(&format!("# Lisk\n\n{heading}")), "{live}");
    assert!(live.ends_with(&format!(")\n\n{text}\n")), "{live}");
    assert_eq!(memory_items(&host, &r.id)[0]["state"], "answered");
    // A second approve adds nothing.
    ok(&answer(&mut bridge, id, "approve", Some(h)).await);
    assert_eq!(host.read_memory().unwrap(), live);

    // A crash after the rename, before the answer: the block is there, the
    // item is open. Approve answers it and appends nothing more.
    let crashed = task(&mut bridge, "Pick a region", Some("- Region: eu-west-1.")).await;
    ok(&end(&mut bridge, &crashed, "done", json!({})).await);
    let item = memory_items(&host, &crashed.id).remove(0);
    let block = format!(
        "## Pick a region ({}, 2026-01-01)\n\n- Region: eu-west-1.\n",
        crashed.id
    );
    host.write_memory(&format!("{live}\n{block}"));
    let written = host.read_memory().unwrap();
    let h = item["memory"]["hash"].as_str().unwrap();
    ok(&answer(
        &mut bridge,
        item["id"].as_str().unwrap(),
        "approve",
        Some(h),
    )
    .await);
    assert_eq!(host.read_memory().unwrap(), written);
    assert_eq!(memory_items(&host, &crashed.id)[0]["state"], "answered");

    // Later moves the item down; reject answers it and writes nothing.
    let other = task(&mut bridge, "Skip it", Some("- Not worth keeping.")).await;
    ok(&end(&mut bridge, &other, "done", json!({})).await);
    let id = memory_items(&host, &other.id)[0]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    ok(&answer(&mut bridge, &id, "later", None).await);
    let later = memory_items(&host, &other.id).remove(0);
    assert!(
        later["laterAt"].is_u64() && later["state"] == "open",
        "{later}"
    );
    ok(&answer(&mut bridge, &id, "reject", None).await);
    let rejected = memory_items(&host, &other.id).remove(0);
    assert_eq!(
        (rejected["state"].as_str(), rejected["answer"].as_str()),
        (Some("answered"), Some("reject"))
    );
    assert_eq!(host.read_memory().unwrap(), written);
    assert!(!host.log().contains(SENTINEL), "the text reached the log");
}

#[tokio::test]
async fn auto_is_the_default_and_appends_at_done_and_falls_back_to_an_item_past_the_cap() {
    let server = Server::start();
    let mut host = Host::new("auto", "");
    let mut bridge = host.bridge(&server, true).await;
    let r = task(
        &mut bridge,
        "Note the freeze",
        Some("- Friday deploy freeze."),
    )
    .await;
    ok(&end(&mut bridge, &r, "done", json!({})).await);
    assert!(memory_items(&host, &r.id).is_empty(), "auto makes no item");
    let live = host.read_memory().expect("appended");
    assert!(
        live.starts_with(&format!("## Note the freeze ({}, ", r.id)),
        "{live}"
    );
    assert!(live.ends_with(")\n\n- Friday deploy freeze.\n"), "{live}");

    // Past 16 KiB the append is refused: the item asks, with the reason.
    host.write_memory(&"x".repeat((16 << 10) - 10));
    let full = task(&mut bridge, "Too much", Some("- One more fact than fits.")).await;
    ok(&end(&mut bridge, &full, "done", json!({})).await);
    let item = memory_items(&host, &full.id).remove(0);
    let summary = item["summary"].as_str().unwrap();
    assert!(
        summary.starts_with("Not added automatically: "),
        "{summary}"
    );
    assert!(summary.contains("the cap is 16 KiB"), "{summary}");
    assert_eq!(item["answers"][0], "approve");
    assert!(log_has(&host, "workspace memory appended without review"));
}

fn log_has(host: &Host, needle: &str) -> bool {
    host.log().contains(needle)
}

#[tokio::test]
async fn an_oversize_section_has_no_approve_and_a_full_file_is_conflict() {
    let server = Server::start();
    let mut host = Host::new("caps", "memory = \"review\"");
    let mut bridge = host.bridge(&server, true).await;
    let big = "y".repeat((4 << 10) + 1);
    let r = task(&mut bridge, "Write a lot", Some(&big)).await;
    ok(&end(&mut bridge, &r, "done", json!({})).await);
    let item = memory_items(&host, &r.id).remove(0);
    assert_eq!(item["answers"], json!(["reject", "later"]));
    assert!(
        item["summary"].as_str().unwrap().contains("4097 bytes"),
        "{item}"
    );
    let h = item["memory"]["hash"].as_str().unwrap();
    let e = error_of(
        &answer(
            &mut bridge,
            item["id"].as_str().unwrap(),
            "approve",
            Some(h),
        )
        .await,
    );
    assert_eq!(e.code, ErrorCode::BadRequest, "{e:?}");

    let full = "z".repeat((16 << 10) - 10);
    host.write_memory(&full);
    let small = task(
        &mut bridge,
        "Add a line",
        Some("- A line that does not fit."),
    )
    .await;
    ok(&end(&mut bridge, &small, "done", json!({})).await);
    let item = memory_items(&host, &small.id).remove(0);
    let h = item["memory"]["hash"].as_str().unwrap();
    let e = error_of(
        &answer(
            &mut bridge,
            item["id"].as_str().unwrap(),
            "approve",
            Some(h),
        )
        .await,
    );
    assert_eq!(e.code, ErrorCode::Conflict, "{e:?}");
    assert!(e.message.contains("bytes; the cap is 16 KiB"), "{e:?}");
    assert!(e.message.contains("WORKSPACE.md would be 16,"), "{e:?}");
    assert_eq!(
        memory_items(&host, &small.id)[0]["state"],
        "open",
        "the item stays open"
    );
    assert_eq!(host.read_memory().unwrap(), full);
}

#[tokio::test]
async fn the_section_is_read_before_the_worktree_goes_and_off_reads_nothing() {
    let server = Server::start();
    let mut host = Host::new("worktree", "memory = \"review\"");
    let mut bridge = host.bridge(&server, true).await;
    let (r, result) = create(&mut bridge, json!({"repo": "app", "branch": "memory-end"})).await;
    ok(&result);
    let r = r.expect("task-saved");
    let worktree = dir_of(&r);
    assert!(worktree.join(".git").is_file(), "a worktree");
    write_section(&r, "- The app's tests need Postgres 16.");
    ok(&end(&mut bridge, &r, "done", json!({"worktree": "remove"})).await);
    assert!(!worktree.exists(), "the worktree is gone");
    let item = memory_items(&host, &r.id).remove(0);
    assert_eq!(
        item["memory"]["text"],
        "- The app's tests need Postgres 16."
    );
    drop(bridge);
    off(&server).await;
}

async fn off(server: &Server) {
    let mut host = Host::new("off", "memory = \"off\"");
    let mut bridge = host.bridge(server, false).await;
    let r = task(&mut bridge, "Quiet", Some("- Never proposed.")).await;
    ok(&end(&mut bridge, &r, "done", json!({})).await);
    assert!(memory_items(&host, &r.id).is_empty());
    assert!(host.read_memory().is_none());
}
