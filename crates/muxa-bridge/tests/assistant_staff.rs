//! A workspace's staff (ADR 0051) end to end: a real daemon on a throwaway
//! tmux server runs requests as named workers, each the fake agent of
//! `data/assistant-agent.py` started as `claude --agent NAME`. A request
//! names a member or goes to the lead; each member's notes land in its own
//! file; worker descriptions, purposes and notes never reach the daemon's log.
#![cfg(feature = "tmux-integration")]

mod common;

use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use common::Server;
use common::daemon::{Conn, Daemon};
use muxa_protocol::{AssistantRequest, Delivery, ErrorCode, Message, StepBy, WorkItem};
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
                "[assistant]\nenabled = true\n\
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
