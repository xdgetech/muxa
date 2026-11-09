//! The overall assistant's plans (ADR 0047 slices B3 and B4) end to end: a
//! real daemon on a throwaway tmux server runs overall runs with the fake
//! agent of `data/assistant-agent.py`. One catch-up and one occurrence of
//! `plans`, the plan check and its fail prompt, an open run conflicts, a
//! budget stops a scheduled plan with a reason, no plan text in the
//! daemon's log, and the windows that were there do not change.
#![cfg(feature = "tmux-integration")]

mod common;

use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use common::Server;
use common::daemon::{Conn, Daemon};
use jiff::{Timestamp, ToSpan, tz::TimeZone};
use muxa_protocol::{ChangeKind, Delivery, ErrorCode, Message, Plan, WorkItem};
use serde_json::{Value, json};

const AGENT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/data/assistant-agent.py");
/// Plan texts that must never reach the daemon's log (ADR 0047 §9).
const MARKS: [&str; 3] = ["why-mark-4a7e", "next-mark-91c0", "notes-mark-d35b"];
const FAIL_PROMPT: &str = "The plan did not pass the check";
/// The midnight plan, whose last occurrence the test sets three days back.
const CATCH_UP: &str = "0 0 * * *";

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
    /// `Overall` is the overall workspace with `overall` in its table; `w`
    /// delegates and shares summaries.
    fn new(name: &str, overall: &str) -> Host {
        let base = std::env::temp_dir().join(format!("muxa-plan-it-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        for d in ["overall", "w", "state/assistant", "steer", "bin"] {
            std::fs::create_dir_all(base.join(d)).unwrap();
        }
        let base = base.canonicalize().unwrap();
        let claude = base.join("bin/claude");
        std::fs::write(&claude, format!("#!/bin/sh\nexec python3 {AGENT} \"$@\"\n")).unwrap();
        std::fs::set_permissions(&claude, std::fs::Permissions::from_mode(0o755)).unwrap();
        let overall = match overall {
            "" => String::new(),
            table => format!("[workspace.assistant]\n{table}\n"),
        };
        std::fs::write(
            base.join("workspaces.toml"),
            format!(
                "[assistant]\nenabled = true\n\
                 [[workspace]]\nname = \"Overall\"\nroot = \"{o}\"\n{overall}\
                 [[workspace]]\nname = \"w\"\nroot = \"{w}\"\n\
                 [workspace.assistant]\ndelegate = true\n",
                o = base.join("overall").display(),
                w = base.join("w").display(),
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

    fn json(&self, path: PathBuf) -> Value {
        serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
    }

    fn record(&self, id: &str) -> Value {
        self.json(self.base.join(format!("state/{id}.json")))
    }

    /// The plan's file, with the host's own fields.
    fn plan_file(&self, id: &str) -> Value {
        self.json(self.base.join(format!("state/assistant/plans/{id}.json")))
    }

    fn tagged(&self, task: &str) -> bool {
        self.server
            .tmux(&["list-sessions", "-F", "#{@muxa_task}"])
            .lines()
            .any(|t| t == task)
    }

    /// The midnight plan was last handled three days ago.
    fn missed_three_days(&self) {
        let last = Timestamp::now().to_zoned(TimeZone::UTC).date() - 3.days();
        let state = json!({"version": 1, "last": {format!("{CATCH_UP} UTC"): last.to_string()}});
        std::fs::write(
            self.base.join("state/assistant/schedule.json"),
            state.to_string(),
        )
        .unwrap();
    }
}

fn error_of(answer: &[Message]) -> (ErrorCode, String) {
    match answer.last() {
        Some(Message::Result { error: Some(e), .. }) => (e.code, e.message.clone()),
        other => panic!("expected an error: {other:?}"),
    }
}

async fn plans(conn: &mut Conn) -> Vec<Plan> {
    let listed = conn.request(json!({"type": "list-inbox"})).await;
    match listed.first() {
        Some(Message::InboxList { plans, .. }) => plans.clone(),
        other => panic!("{other:?}"),
    }
}

/// Waits until `cond` holds for the listed plans, and returns them.
async fn until_plans(
    conn: &mut Conn,
    h: &Host,
    what: &str,
    wait: Duration,
    cond: impl Fn(&[Plan]) -> bool,
) -> Vec<Plan> {
    let deadline = Instant::now() + wait;
    loop {
        let listed = plans(conn).await;
        if cond(&listed) {
            return listed;
        }
        assert!(
            Instant::now() < deadline,
            "{what}: {listed:?}; turns: {:?}",
            h.turns()
        );
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
}

/// A Work item in `w`, pinned.
async fn pinned_item(conn: &mut Conn) -> WorkItem {
    let saved = conn
        .request(
            json!({"type": "capture", "clientId": "c-plan", "text": "Pick a host.",
                        "workspace": "w"}),
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
    let Some(Message::CaptureSaved { work: Some(w), .. }) = sorted.first() else {
        panic!("{sorted:?}")
    };
    let pinned = conn
        .request(json!({"type": "update-work", "work": w.id, "rev": w.rev, "action": "pin"}))
        .await;
    match pinned.first() {
        Some(Message::WorkSaved { entry, .. }) => entry.clone(),
        other => panic!("{other:?}"),
    }
}

fn plan_json(work: &str) -> Value {
    json!({"version": 1, "plan": "@",
           "top": [{"work": work, "why": format!("Pinned. {}", MARKS[0]),
                    "next": format!("Compare two hosts. {}", MARKS[1]),
                    "minutes": {"low": 20, "estimated": 30, "high": 60},
                    "basis": [{"kind": "manual", "text": "Pinned"}]}],
           "notes": format!("# Overall\n- {}\n", MARKS[2])})
}

/// The next whole minute at least 70 seconds away, as `minute hour` in
/// UTC, and when it is. Never midnight: that is the catch-up's.
fn soon() -> (String, Timestamp) {
    let after = (Timestamp::now() + 70.seconds()).as_second();
    let mut due = Timestamp::from_second((after / 60 + 1) * 60).unwrap();
    let mut z = due.to_zoned(TimeZone::UTC);
    if (z.hour(), z.minute()) == (0, 0) {
        due += 60.seconds();
        z = due.to_zoned(TimeZone::UTC);
    }
    (format!("{} {} * * *", z.minute(), z.hour()), due)
}

#[tokio::test(flavor = "multi_thread")]
async fn scheduled_plans_run_once_conflict_while_open_and_stop_at_a_budget() {
    let (next, next_at) = soon();
    let h = Host::new(
        "schedule",
        &format!(
            "overall = true\nmax_runs_per_day = 1\nplans = [\"{CATCH_UP}\", \"{next}\"]\n\
             timezone = \"UTC\""
        ),
    );
    h.missed_three_days();
    h.steer("stall", &json!(true));
    let mut daemon = h.daemon().await;
    let mut conn = daemon.connect().await;
    let before = h.server.layout();

    // The first tick: three missed midnights make one plan.
    let listed = until_plans(
        &mut conn,
        &h,
        "the catch-up",
        Duration::from_secs(60),
        |p| {
            p.first()
                .is_some_and(|p| p.delivery == Delivery::Processing)
        },
    )
    .await;
    assert_eq!(listed.len(), 1, "one catch-up: {listed:?}");
    let first = listed[0].clone();
    let midnight = Timestamp::now()
        .to_zoned(TimeZone::UTC)
        .start_of_day()
        .unwrap();
    assert_eq!(first.at as i64, midnight.timestamp().as_second());
    let run = first.run.clone().expect("a run");
    assert_eq!(h.record(&run)["kind"], "assistant-run");
    assert_eq!(h.record(&run)["assistant"]["plan"], json!(first.id));

    // One plan run at a time.
    let (code, message) = error_of(&conn.request(json!({"type": "plan-now"})).await);
    assert_eq!(code, ErrorCode::Conflict);
    assert!(message.contains("being made"), "{message}");

    // A wrong first plan gets the fail prompt; the fixed one is stored.
    let item = pinned_item(&mut conn).await;
    h.steer("first.json", &plan_json("w-ffffffff"));
    h.steer("fixed.json", &plan_json(&item.id));
    h.steer("release", &json!(true));
    let listed = until_plans(&mut conn, &h, "the plan", Duration::from_secs(120), |p| {
        p.first().is_some_and(|p| p.delivery == Delivery::Resulted)
    })
    .await;
    let done = &listed[0];
    assert_eq!(done.id, first.id);
    assert_eq!(done.top.len(), 1);
    assert_eq!(done.top[0].work.as_deref(), Some(item.id.as_str()));
    assert_eq!(done.workspaces, ["w"]);
    assert_eq!(done.changed.len(), 1);
    assert_eq!(done.changed[0].change, ChangeKind::Entered);
    assert!(done.notes.as_ref().unwrap().text.contains(MARKS[2]));
    let turns = h.turns();
    assert_eq!(
        turns.iter().filter(|t| t.contains(FAIL_PROMPT)).count(),
        1,
        "{turns:?}"
    );
    let dir = PathBuf::from(h.record(&run)["dir"].as_str().unwrap());
    assert!(dir.starts_with(h.base.join("overall/assistant")));
    let check = std::fs::read_to_string(dir.join(".muxa/checks/1.log")).unwrap();
    assert!(
        check.contains("top[0].work is not an open Work item"),
        "{check}"
    );
    let brief = std::fs::read_to_string(dir.join(".muxa/assistant/BRIEF.md")).unwrap();
    assert!(brief.contains("### Workspace \"w\""), "{brief}");
    assert!(brief.ends_with(&format!("muxa-plan: {}\n", first.id)));
    let task_md = std::fs::read_to_string(dir.join(".muxa/TASK.md")).unwrap();
    assert!(task_md.contains(".muxa/assistant/BRIEF.md"));
    let deadline = Instant::now() + Duration::from_secs(30);
    while h.tagged(&run) {
        assert!(
            Instant::now() < deadline,
            "the published run's session stays"
        );
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
    assert_eq!(h.record(&run)["status"], "done");

    // The day's one run is used: plan-now names the limit.
    let (code, message) = error_of(&conn.request(json!({"type": "plan-now"})).await);
    assert_eq!(code, ErrorCode::Conflict);
    assert!(message.contains("max_runs_per_day"), "{message}");

    // The next occurrence comes at its time, and the budget stops it with a reason.
    let left = next_at.as_second() - Timestamp::now().as_second();
    let wait = Duration::from_secs(left.max(0) as u64 + 60);
    let listed = until_plans(&mut conn, &h, "the next occurrence", wait, |p| p.len() == 2).await;
    let stopped = &listed[0];
    assert_eq!(stopped.at as i64, next_at.as_second());
    assert_eq!(stopped.delivery, Delivery::Failed);
    assert_eq!(stopped.run, None);
    let reason = h.plan_file(&stopped.id)["reason"]
        .as_str()
        .unwrap()
        .to_owned();
    assert!(reason.contains("max_runs_per_day"), "{reason}");
    assert_eq!(listed[1].id, first.id, "newest first");
    tokio::time::sleep(Duration::from_secs(5)).await;
    assert_eq!(plans(&mut conn).await.len(), 2, "each occurrence once");

    let after = h.server.layout();
    for line in before.lines() {
        assert!(after.lines().any(|l| l == line), "{line} changed:\n{after}");
    }
    let log = daemon.log();
    for mark in MARKS {
        assert!(!log.contains(mark), "the daemon log holds {mark}:\n{log}");
    }
    assert!(
        log.contains("plan due"),
        "schedule times are logged:\n{log}"
    );
    daemon.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn plan_now_without_an_overall_workspace_is_unsupported() {
    let h = Host::new("none", "");
    let mut daemon = h.daemon().await;
    let mut conn = daemon.connect().await;
    let (code, message) = error_of(&conn.request(json!({"type": "plan-now"})).await);
    assert_eq!(code, ErrorCode::Unsupported);
    assert!(message.contains("overall = true"), "{message}");
    let (code, _) = error_of(
        &conn
            .request(json!({"type": "plan-now", "minutes": 0}))
            .await,
    );
    assert_eq!(code, ErrorCode::BadRequest);
    assert!(plans(&mut conn).await.is_empty());
    daemon.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn plan_now_makes_a_plan_with_the_users_minutes() {
    let h = Host::new("now", "overall = true\nplans = []");
    let mut daemon = h.daemon().await;
    let mut conn = daemon.connect().await;
    let item = pinned_item(&mut conn).await;
    h.steer("first.json", &plan_json(&item.id));
    let answer = conn
        .request(json!({"type": "plan-now", "minutes": 45}))
        .await;
    let Some(Message::PlanSaved { entry, .. }) = answer.first() else {
        panic!("{answer:?}")
    };
    assert!(matches!(
        answer.last(),
        Some(Message::Result { error: None, .. })
    ));
    assert_eq!(entry.minutes, Some(45));
    let id = entry.id.clone();
    let listed = until_plans(&mut conn, &h, "the plan", Duration::from_secs(120), |p| {
        p.first().is_some_and(|p| p.delivery == Delivery::Resulted)
    })
    .await;
    assert_eq!(listed[0].id, id);
    let dir = PathBuf::from(
        h.record(listed[0].run.as_ref().unwrap())["dir"]
            .as_str()
            .unwrap(),
    );
    let brief = std::fs::read_to_string(dir.join(".muxa/assistant/BRIEF.md")).unwrap();
    assert!(brief.contains("about 45 minutes"));
    daemon.shutdown().await;
}
