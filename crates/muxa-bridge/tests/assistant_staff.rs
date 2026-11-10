//! A workspace's staff (ADR 0051) end to end: a real daemon on a throwaway
//! tmux server runs requests as named workers, each the fake agent of
//! `data/assistant-agent.py` started as `claude --agent NAME`. A request
//! names a member or goes to the lead; each member's notes land in its own
//! file; a hand-off is checked and kept on its step; worker descriptions,
//! purposes and notes never reach the daemon's log.
#![cfg(feature = "tmux-integration")]

mod common;

use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use common::Server;
use common::daemon::{Conn, Daemon};
use muxa_protocol::{
    AssistantRequest, Delivery, ErrorCode, InboxAction, InboxItem, ItemKind, ItemState, Message,
    StepBy, WorkItem,
};
use serde_json::{Value, json};

const AGENT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/data/assistant-agent.py");
const LEAD: &str = "lisk-chief-of-staff";
const STRATEGIST: &str = "lisk-strategist";
const ARCHITECT: &str = "lisk-architect";
/// Content that must never reach the daemon's log.
const MARKS: [&str; 5] = [
    "description-mark-41c2",
    "purpose-mark-7f30",
    "notes-mark-d81e",
    "handoff-mark-2b9a",
    "capture-mark-6e04",
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
    /// Workspace `lisk` with a staff of three and `extra` in its
    /// `[workspace.assistant]`.
    fn new(name: &str, extra: &str) -> Host {
        Host::with(name, extra, "")
    }

    /// As [`Host::new`], with `host` in `[assistant]`.
    fn with(name: &str, extra: &str, host: &str) -> Host {
        let base =
            std::env::temp_dir().join(format!("muxa-staff-it-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        for d in ["lisk", "state", "steer", "bin", "claude"] {
            std::fs::create_dir_all(base.join(d)).unwrap();
        }
        let base = base.canonicalize().unwrap();
        // Workers start `claude` from PATH; the program's name is how the adapter is known.
        let claude = base.join("bin/claude");
        std::fs::write(&claude, format!("#!/bin/sh\nexec python3 {AGENT} \"$@\"\n")).unwrap();
        std::fs::set_permissions(&claude, std::fs::Permissions::from_mode(0o755)).unwrap();
        let worker = |name: &str, job: &str| {
            format!(
                "[[worker]]\nname = \"{name}\"\nagent = \"claude-code\"\nargs = [\"--agent\", \"{name}\"]\n\
                 description = \"{job} {}\"\n",
                MARKS[0]
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
                 staff = [\"{LEAD}\", \"{STRATEGIST}\", \"{ARCHITECT}\"]\n{extra}\n\
                 {}{}{}",
                worker(LEAD, "Chief of staff."),
                worker(STRATEGIST, "Strategist."),
                worker(ARCHITECT, "Architect."),
                root = base.join("lisk").display(),
                bin = base.join("bin").display(),
                steer = base.join("steer").display(),
                // The fake agent runs tmux and python3 wherever this host has them.
                path = std::env::var("PATH").unwrap_or_default(),
                // A named worker's run is delivered only to a process its
                // adapter binds: the fake agent keeps a session file here.
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

    fn dir(&self, task: &str) -> PathBuf {
        PathBuf::from(self.record(task)["dir"].as_str().unwrap())
    }

    fn notes(&self, rel: &str) -> Option<String> {
        std::fs::read_to_string(self.base.join("lisk").join(rel)).ok()
    }
}

fn result(summary: &str) -> Value {
    json!({"version": 1, "request": "@", "requestVersion": 0, "summary": summary,
           "findings": [{"text": "Option B fits.", "sources": [{"url": "https://example.com/b"}]}]})
}

fn with(mut v: Value, field: &str, value: Value) -> Value {
    v[field] = value;
    v
}

fn error_of(answer: &[Message]) -> (ErrorCode, String) {
    match answer.last() {
        Some(Message::Result { error: Some(e), .. }) => (e.code, e.message.clone()),
        other => panic!("expected an error: {other:?}"),
    }
}

async fn work_item(conn: &mut Conn, client: &str) -> WorkItem {
    let text = format!("Decide the roadmap. {}", MARKS[4]);
    let saved = conn
        .request(json!({"type": "capture", "clientId": client, "text": text, "workspace": "lisk"}))
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

async fn ask_raw(conn: &mut Conn, work: &WorkItem, worker: Option<&str>) -> Vec<Message> {
    let mut req = json!({"type": "request-assistant", "work": work.id, "rev": work.rev,
                         "scope": "compare", "purpose": format!("Pick one. {}", MARKS[1])});
    if let Some(w) = worker {
        req["worker"] = json!(w);
    }
    conn.request(req).await
}

async fn ask(conn: &mut Conn, work: &WorkItem, worker: Option<&str>) -> AssistantRequest {
    let answer = ask_raw(conn, work, worker).await;
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

fn assert_no_marks(daemon: &Daemon) {
    let log = daemon.log();
    for mark in MARKS {
        assert!(!log.contains(mark), "the daemon log holds {mark}:\n{log}");
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_request_runs_as_the_named_member_or_the_lead_with_its_own_notes() {
    let h = Host::new("members", "notes = \"auto\"");
    let lead_notes = format!("# Shared\n\n- {}\n", MARKS[2]);
    let own_notes = format!("# Strategist\n\n- {}\n", MARKS[2]);
    h.steer(
        &format!("first.{LEAD}.json"),
        &with(result("Lead's answer."), "notes", json!(lead_notes)),
    );
    h.steer(
        &format!("first.{STRATEGIST}.json"),
        &with(result("Strategist's answer."), "notes", json!(own_notes)),
    );
    let mut daemon = h.daemon().await;
    let mut conn = daemon.connect().await;
    let before = h.server.layout();

    // A worker not on the staff is refused with the staff's names.
    let item = work_item(&mut conn, "c-staff-1").await;
    let (code, message) = error_of(&ask_raw(&mut conn, &item, Some("lisk-sales")).await);
    assert_eq!(code, ErrorCode::BadRequest);
    assert!(
        message.contains(&format!("{LEAD}, {STRATEGIST}, {ARCHITECT}")),
        "{message}"
    );

    // No worker: the lead takes it, and its notes are the workspace's.
    let q = ask(&mut conn, &item, None).await;
    assert_eq!(q.steps.len(), 1);
    assert_eq!(
        (q.steps[0].worker.as_str(), q.steps[0].by),
        (LEAD, StepBy::Lead)
    );
    let run = q.run.clone().unwrap();
    assert_eq!(h.record(&run)["worker"], LEAD);
    let done = until(&mut conn, &h, &q.id, Delivery::Resulted).await;
    assert_eq!(done.results[0].worker.as_deref(), Some(LEAD));
    assert_eq!(done.results[0].step, Some(1));
    assert_eq!(done.steps[0].delivery, Delivery::Resulted);
    let task_md = std::fs::read_to_string(h.dir(&run).join(".muxa/TASK.md")).unwrap();
    assert!(
        task_md.contains(&format!("\nYou are {LEAD}, a member")),
        "{task_md}"
    );
    assert!(task_md.contains(&format!("- {STRATEGIST}: Strategist.")));
    assert!(task_md.contains("This is step 1 of at most 3"));
    assert_eq!(
        h.notes(".muxa/assistant/NOTES.md"),
        Some(lead_notes.clone())
    );

    // A named member: its own notes file, never NOTES.md.
    let other = work_item(&mut conn, "c-staff-2").await;
    let q = ask(&mut conn, &other, Some(STRATEGIST)).await;
    assert_eq!(
        (q.steps[0].worker.as_str(), q.steps[0].by),
        (STRATEGIST, StepBy::User)
    );
    let run = q.run.clone().unwrap();
    let dir = h.dir(&run);
    let done = until(&mut conn, &h, &q.id, Delivery::Resulted).await;
    assert_eq!(done.results[0].worker.as_deref(), Some(STRATEGIST));
    let own = format!(".muxa/assistant/staff/{STRATEGIST}/NOTES.md");
    assert_eq!(h.notes(&own), Some(own_notes));
    assert_eq!(
        h.notes(".muxa/assistant/NOTES.md"),
        Some(lead_notes.clone())
    );
    // The member read the shared notes as data, and its own copy.
    let copy = std::fs::read_to_string(dir.join(".muxa/assistant/NOTES.md")).unwrap();
    assert_eq!(copy, lead_notes);
    assert!(dir.join(&own).is_file());
    let task_md = std::fs::read_to_string(dir.join(".muxa/TASK.md")).unwrap();
    assert!(task_md.contains("You cannot change it"), "{task_md}");

    // inbox-list carries the staff, its lead and the member's notes time.
    let listed = conn.request(json!({"type": "list-inbox"})).await;
    let Some(Message::InboxList {
        assistant: Some(info),
        ..
    }) = listed.first()
    else {
        panic!("{listed:?}")
    };
    let staff = &info.delegates[0].staff;
    assert_eq!(
        staff
            .iter()
            .map(|m| (m.worker.as_str(), m.lead))
            .collect::<Vec<_>>(),
        [(LEAD, true), (STRATEGIST, false), (ARCHITECT, false)]
    );
    assert!(staff[1].notes_at.is_some() && staff[2].notes_at.is_none());
    assert!(staff[0].description.contains("Chief of staff."));

    let after = h.server.layout();
    for line in before.lines() {
        assert!(after.lines().any(|l| l == line), "{line} changed:\n{after}");
    }
    assert_no_marks(&daemon);
    daemon.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_hand_off_is_checked_and_kept_on_its_step() {
    let h = Host::new("handoff", "");
    let purpose = format!("Compare the two options. {}", MARKS[3]);
    // The lead first hands the request to itself, then to the strategist.
    let handing = |to: &str| {
        with(
            result("Two options remain."),
            "handoff",
            json!({"worker": to, "purpose": purpose}),
        )
    };
    h.steer(&format!("first.{LEAD}.json"), &handing(LEAD));
    h.steer(&format!("fixed.{LEAD}.json"), &handing(STRATEGIST));
    let mut daemon = h.daemon().await;
    let mut conn = daemon.connect().await;
    let item = work_item(&mut conn, "c-handoff").await;
    let q = ask(&mut conn, &item, None).await;
    let run = q.run.clone().unwrap();
    let done = until(&mut conn, &h, &q.id, Delivery::Resulted).await;
    let log = std::fs::read_to_string(h.dir(&run).join(".muxa/checks/1.log")).unwrap();
    assert!(log.contains("this run's own worker"), "{log}");
    assert!(!log.contains(MARKS[3]), "{log}");
    let task_md = std::fs::read_to_string(h.dir(&run).join(".muxa/TASK.md")).unwrap();
    assert!(task_md.contains("### Hand-off"), "{task_md}");
    // The hand-off waits on its step: approving it starts the next one.
    assert_eq!(done.steps.len(), 1);
    let handoff = done.steps[0].handoff.clone().expect("the step keeps it");
    assert_eq!(
        (handoff.worker.as_str(), handoff.purpose.as_str()),
        (STRATEGIST, purpose.as_str())
    );
    assert_eq!(
        (done.results[0].step, done.results[0].stale),
        (Some(1), false)
    );
    assert_no_marks(&daemon);
    daemon.shutdown().await;
}

/// A result that hands the request to `to`, with a purpose that must stay
/// out of the log.
fn handing(summary: &str, to: &str) -> Value {
    with(
        result(summary),
        "handoff",
        json!({"worker": to, "purpose": format!("Go on. {}", MARKS[3])}),
    )
}

async fn ask_full(conn: &mut Conn, work: &WorkItem) -> (WorkItem, AssistantRequest) {
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

/// The request once `done` holds for it.
async fn until_q(
    conn: &mut Conn,
    h: &Host,
    id: &str,
    what: &str,
    done: impl Fn(&AssistantRequest) -> bool,
) -> AssistantRequest {
    let deadline = Instant::now() + RUN_WAIT;
    loop {
        let q = request(conn, id).await;
        if done(&q) {
            return q;
        }
        assert!(
            Instant::now() < deadline,
            "not {what}: {q:?}; turns: {:?}",
            h.turns()
        );
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
}

/// Step `step`'s result is stored.
fn resulted(step: u32) -> impl Fn(&AssistantRequest) -> bool {
    move |q| q.delivery == Delivery::Resulted && q.steps.last().is_some_and(|s| s.step == step)
}

async fn handoff_items(conn: &mut Conn, request: &str) -> Vec<InboxItem> {
    let listed = conn.request(json!({"type": "list-inbox"})).await;
    let Some(Message::InboxList { items, .. }) = listed.first() else {
        panic!("{listed:?}")
    };
    items
        .iter()
        .filter(|i| i.kind == ItemKind::Handoff)
        .filter(|i| i.handoff.as_ref().is_some_and(|p| p.request == request))
        .cloned()
        .collect()
}

/// The `handoff` item of `request`'s `version` and `step`, once it exists.
async fn until_item(conn: &mut Conn, request: &str, version: u64, step: u32) -> InboxItem {
    let deadline = Instant::now() + RUN_WAIT;
    loop {
        let found = handoff_items(conn, request).await.into_iter().find(|i| {
            i.handoff
                .as_ref()
                .is_some_and(|p| p.version == version && p.step == step)
        });
        if let Some(item) = found {
            return item;
        }
        assert!(
            Instant::now() < deadline,
            "no hand-off item for step {step}"
        );
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
}

async fn approve(conn: &mut Conn, item: &str) -> Vec<Message> {
    conn.request(json!({"type": "update-inbox", "entry": item, "action": "approve"}))
        .await
}

/// The request an approve started, and the item it answered.
fn approved(answer: &[Message]) -> (AssistantRequest, InboxItem) {
    match answer {
        [
            Message::WorkSaved {
                request: Some(q), ..
            },
            Message::InboxSaved { item: Some(i), .. },
            Message::Result { error: None, .. },
        ] => ((**q).clone(), (**i).clone()),
        other => panic!("approve: {other:?}"),
    }
}

fn assert_layout_kept(before: &str, h: &Host) {
    let after = h.server.layout();
    for line in before.lines() {
        assert!(after.lines().any(|l| l == line), "{line} changed:\n{after}");
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_three_step_chain_under_review_makes_two_items_and_three_runs() {
    let h = Host::new("review", "");
    h.steer(
        &format!("first.{LEAD}.json"),
        &handing("Two options.", STRATEGIST),
    );
    h.steer(
        &format!("first.{STRATEGIST}.json"),
        &handing("B is cheaper.", ARCHITECT),
    );
    h.steer(
        &format!("first.{ARCHITECT}.json"),
        &result("B fits the system."),
    );
    let mut daemon = h.daemon().await;
    let mut conn = daemon.connect().await;
    let before = h.server.layout();
    let work = work_item(&mut conn, "c-review").await;
    let q = ask(&mut conn, &work, None).await;
    until_q(&mut conn, &h, &q.id, "step 1 resulted", resulted(1)).await;

    // Step 1's hand-off waits for review.
    let first = until_item(&mut conn, &q.id, 1, 1).await;
    assert_eq!(first.state, ItemState::Open);
    assert_eq!(
        first.answers,
        [
            InboxAction::Approve,
            InboxAction::Reject,
            InboxAction::Later
        ]
    );
    assert!(
        first
            .question
            .starts_with(&format!("{LEAD} asks {STRATEGIST} to: Go on.")),
        "{}",
        first.question
    );
    let (started, answered) = approved(&approve(&mut conn, &first.id).await);
    assert_eq!(answered.answer, Some(InboxAction::Approve));
    let step2 = started.steps.last().cloned().unwrap();
    assert_eq!(
        (step2.step, step2.worker.as_str(), step2.by),
        (2, STRATEGIST, StepBy::Handoff)
    );
    let run2 = step2.run.clone().expect("step 2 has its run");
    // A second approve starts nothing new and returns the same step.
    let (again, _) = approved(&approve(&mut conn, &first.id).await);
    assert_eq!(again.steps.len(), 2);
    assert_eq!(again.steps[1].run.as_deref(), Some(run2.as_str()));

    until_q(&mut conn, &h, &q.id, "step 2 resulted", resulted(2)).await;
    let task_md = std::fs::read_to_string(h.dir(&run2).join(".muxa/TASK.md")).unwrap();
    assert!(
        task_md.contains(&format!("{LEAD} handed the request to you")),
        "{task_md}"
    );
    assert!(
        task_md.contains("Step 1, lisk-chief-of-staff:"),
        "{task_md}"
    );
    let second = until_item(&mut conn, &q.id, 1, 2).await;
    approved(&approve(&mut conn, &second.id).await);
    let done = until_q(&mut conn, &h, &q.id, "step 3 resulted", resulted(3)).await;

    // Three runs, one result each, none stale, and no third item.
    let runs: std::collections::HashSet<_> =
        done.steps.iter().filter_map(|s| s.run.clone()).collect();
    assert_eq!(runs.len(), 3);
    let mut parts: Vec<_> = done
        .results
        .iter()
        .map(|r| (r.step, r.worker.as_deref(), r.stale))
        .collect();
    parts.sort();
    assert_eq!(
        parts,
        [
            (Some(1), Some(LEAD), false),
            (Some(2), Some(STRATEGIST), false),
            (Some(3), Some(ARCHITECT), false)
        ]
    );
    assert!(done.steps[2].handoff.is_none());
    let items = handoff_items(&mut conn, &q.id).await;
    assert_eq!(items.len(), 2, "{items:?}");
    assert!(items.iter().all(|i| i.state == ItemState::Answered));

    assert_layout_kept(&before, &h);
    assert_no_marks(&daemon);
    daemon.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn auto_runs_the_chain_within_budgets_and_holds_the_step_past_one() {
    let h = Host::with(
        "auto",
        "handoffs = \"auto\"",
        "max_auto_handoffs_per_day = 1",
    );
    h.steer(
        &format!("first.{LEAD}.json"),
        &handing("Two options.", STRATEGIST),
    );
    h.steer(
        &format!("first.{STRATEGIST}.json"),
        &handing("B is cheaper.", ARCHITECT),
    );
    h.steer(
        &format!("first.{ARCHITECT}.json"),
        &result("B fits the system."),
    );
    let mut daemon = h.daemon().await;
    let mut conn = daemon.connect().await;
    let before = h.server.layout();
    let work = work_item(&mut conn, "c-auto").await;
    let q = ask(&mut conn, &work, None).await;

    // Step 2 starts without an item; step 3 is past the day's limit.
    until_q(&mut conn, &h, &q.id, "step 2 resulted", resulted(2)).await;
    let held = until_item(&mut conn, &q.id, 1, 2).await;
    let summary = held.summary.clone().unwrap_or_default();
    assert!(summary.contains("max_auto_handoffs_per_day"), "{summary}");
    let items = handoff_items(&mut conn, &q.id).await;
    assert_eq!(items.len(), 1, "no item for the auto step: {items:?}");
    tokio::time::sleep(Duration::from_secs(5)).await;
    let waiting = request(&mut conn, &q.id).await;
    assert_eq!(waiting.steps.len(), 2, "the host does not try again");
    assert_eq!(waiting.steps[1].by, StepBy::Handoff);

    // The user's approval starts it.
    approved(&approve(&mut conn, &held.id).await);
    let done = until_q(&mut conn, &h, &q.id, "step 3 resulted", resulted(3)).await;
    assert_eq!(done.results.len(), 3);

    assert_layout_kept(&before, &h);
    assert_no_marks(&daemon);
    daemon.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_new_version_ends_the_open_step_and_a_cancel_closes_the_chain() {
    let h = Host::new("ends", "");
    h.steer(
        &format!("first.{LEAD}.json"),
        &handing("Two options.", STRATEGIST),
    );
    h.steer(&format!("first.{STRATEGIST}.json"), &result("B."));
    h.steer(&format!("stall.{STRATEGIST}"), &json!(true));
    let mut daemon = h.daemon().await;
    let mut conn = daemon.connect().await;
    let before = h.server.layout();
    let work = work_item(&mut conn, "c-ends").await;
    let (work, q) = ask_full(&mut conn, &work).await;
    until_q(&mut conn, &h, &q.id, "step 1 resulted", resulted(1)).await;
    let item = until_item(&mut conn, &q.id, 1, 1).await;
    let (started, _) = approved(&approve(&mut conn, &item.id).await);
    let run2 = started.steps[1].run.clone().unwrap();
    until_q(&mut conn, &h, &q.id, "step 2 processing", |q| {
        q.steps.len() == 2 && q.delivery == Delivery::Processing
    })
    .await;

    // A new version ends the open step and starts again at step 1.
    let (work, v2) = ask_full(&mut conn, &work).await;
    assert_eq!((v2.version, v2.steps.len()), (2, 1));
    let deadline = Instant::now() + RUN_WAIT;
    while h.record(&run2)["status"] != "dropped" {
        assert!(Instant::now() < deadline, "step 2 still runs");
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    let v2 = until_q(&mut conn, &h, &q.id, "v2 step 1 resulted", resulted(1)).await;
    assert_eq!(v2.version, 2);
    assert!(
        v2.results
            .iter()
            .filter(|r| r.version == 1)
            .all(|r| r.stale)
    );

    // A newer version resolves v2's waiting item.
    let item2 = until_item(&mut conn, &q.id, 2, 1).await;
    let (_, v3) = ask_full(&mut conn, &work).await;
    assert_eq!(v3.version, 3);
    let deadline = Instant::now() + RUN_WAIT;
    loop {
        let items = handoff_items(&mut conn, &q.id).await;
        let i = items.iter().find(|i| i.id == item2.id).unwrap();
        if i.state == ItemState::Resolved {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "v2's item is still {:?}",
            i.state
        );
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    let (code, _) = error_of(&approve(&mut conn, &item2.id).await);
    assert_eq!(code, ErrorCode::BadRequest);

    // A cancel while v3 waits on its hand-off closes the chain; the result stays.
    until_q(&mut conn, &h, &q.id, "v3 step 1 resulted", resulted(1)).await;
    let item3 = until_item(&mut conn, &q.id, 3, 1).await;
    let cancelled = conn
        .request(json!({"type": "cancel-request", "request": q.id, "version": 3}))
        .await;
    let Some(Message::WorkSaved {
        request: Some(after),
        ..
    }) = cancelled.first()
    else {
        panic!("{cancelled:?}")
    };
    assert_eq!((after.delivery, after.steps.len()), (Delivery::Resulted, 1));
    let items = handoff_items(&mut conn, &q.id).await;
    let closed = items.iter().find(|i| i.id == item3.id).unwrap();
    assert_eq!(closed.state, ItemState::Resolved);
    let (code, _) = error_of(&approve(&mut conn, &item3.id).await);
    assert_eq!(code, ErrorCode::BadRequest);
    tokio::time::sleep(Duration::from_secs(5)).await;
    assert_eq!(request(&mut conn, &q.id).await.steps.len(), 1);

    h.steer("release", &json!(true));
    assert_layout_kept(&before, &h);
    assert_no_marks(&daemon);
    daemon.shutdown().await;
}
