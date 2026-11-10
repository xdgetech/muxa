use std::path::PathBuf;

use muxa_protocol::{CaptureSource, Due, ItemState};
use serde_json::{Value, json};

use super::*;
use crate::assistant::notes::base_of;
use crate::assistant::record::RunFor;
use crate::items::store::tests_item;
use crate::loops::step::CheckEnd;

/// Text the agent wrote: never in a reason or a log line.
const MARK: &str = "plan-mark-2c9a";

fn workspaces(root: &str) -> Workspaces {
    let text = format!(
        "[assistant]\nenabled = true\n\
         [[workspace]]\nname = \"Overall\"\nroot = \"{root}\"\n[workspace.assistant]\noverall = true\n\
         [[workspace]]\nname = \"work\"\nroot = \"{root}\"\n[workspace.assistant]\ndelegate = true\n\
         [[workspace]]\nname = \"closed\"\nroot = \"{root}\"\n[workspace.assistant]\n\
         delegate = true\noverall_requests = \"none\"\n"
    );
    Workspaces::parse(&text, Path::new("w.toml"), None).unwrap()
}

fn work(id: &str) -> WorkItem {
    WorkItem {
        id: id.into(),
        rev: 1,
        title: "t".into(),
        state: WorkState::Open,
        created_at: 1,
        updated_at: 1,
        workspace: Some("work".into()),
        later_at: None,
        merged_into: None,
        pinned_at: None,
        due: None,
        captures: Vec::new(),
        requests: Vec::new(),
        tasks: Vec::new(),
    }
}

fn capture(id: &str, state: CaptureState) -> Capture {
    Capture {
        id: id.into(),
        rev: 1,
        client_id: id.into(),
        text: "x".into(),
        source: CaptureSource::App,
        state,
        created_at: 1,
        updated_at: 1,
        workspace: None,
        work: Vec::new(),
    }
}

fn pinned(id: &str, at: u64) -> WorkItem {
    WorkItem {
        pinned_at: Some(at),
        ..work(id)
    }
}

fn plan_file(top: Vec<Value>, proposals: Vec<Value>) -> Value {
    json!({"version": 1, "plan": "p-0a1b2c3d", "top": top, "proposals": proposals})
}

fn row(key: &str, id: &str) -> Value {
    json!({key: id, "why": format!("Why. {MARK}"), "next": "Next.",
           "minutes": {"low": 5, "estimated": 10, "high": 20}})
}

fn checked(v: &Value) -> Checked {
    let run = PlanRun {
        plan: "p-0a1b2c3d".into(),
        notes_base: base_of(b""),
    };
    file::validate(v.to_string().as_bytes(), &run).unwrap()
}

struct Store {
    work: Vec<WorkItem>,
    items: Vec<InboxItem>,
    captures: Vec<Capture>,
    workspaces: Workspaces,
}

impl Store {
    fn new(work: Vec<WorkItem>) -> Self {
        Self {
            work,
            items: vec![
                tests_item("i-00000001", ItemState::Open),
                tests_item("i-00000002", ItemState::Later),
            ],
            captures: vec![
                capture("c-00000001", CaptureState::Received),
                capture("c-00000002", CaptureState::Sorted),
            ],
            workspaces: workspaces(&std::env::temp_dir().to_string_lossy()),
        }
    }

    fn reasons(&self, v: &Value) -> Vec<String> {
        let facts = Facts {
            work: &self.work,
            items: &self.items,
            captures: &self.captures,
            workspaces: &self.workspaces,
            told: Told::nothing(),
        };
        let reasons = against(&checked(v), &facts);
        for r in &reasons {
            assert!(!r.contains(MARK), "a reason quotes the plan: {r}");
        }
        reasons
    }
}

#[test]
fn rows_and_proposals_that_hold_against_the_store_pass() {
    let s = Store::new(vec![
        pinned("w-00000002", 20),
        pinned("w-00000001", 10),
        work("w-00000003"),
    ]);
    let top = vec![
        row("work", "w-00000001"),
        row("work", "w-00000002"),
        row("item", "i-00000001"),
    ];
    let proposals = vec![
        json!({"kind": "sort", "capture": "c-00000001", "action": "link", "work": "w-00000003"}),
        json!({"kind": "delegate", "work": "w-00000003", "workspace": "work",
               "scope": "research", "purpose": "Find prices.", "reason": "Due."}),
    ];
    assert_eq!(s.reasons(&plan_file(top, proposals)), Vec::<String>::new());
}

#[test]
fn a_pinned_item_must_take_the_first_rows_in_pin_order() {
    let s = Store::new(vec![
        pinned("w-00000002", 20),
        pinned("w-00000001", 10),
        work("w-00000003"),
    ]);
    let swapped = vec![row("work", "w-00000002"), row("work", "w-00000001")];
    let reasons = s.reasons(&plan_file(swapped, vec![]));
    assert_eq!(reasons.len(), 2, "{reasons:?}");
    assert!(reasons[0].starts_with("top[0] must be the pinned Work item w-00000001"));
    let after = vec![row("work", "w-00000003"), row("work", "w-00000001")];
    assert!(!s.reasons(&plan_file(after, vec![])).is_empty());
    let short = vec![row("work", "w-00000001")];
    let reasons = s.reasons(&plan_file(short, vec![]));
    assert!(
        reasons[0].contains("start with the 2 pinned Work items"),
        "{reasons:?}"
    );
    // Four pins: the three oldest fill the plan.
    let four = Store::new(
        (1..=4)
            .map(|n| pinned(&format!("w-0000000{n}"), n))
            .collect(),
    );
    let three = (1..=3)
        .map(|n| row("work", &format!("w-0000000{n}")))
        .collect();
    assert!(four.reasons(&plan_file(three, vec![])).is_empty());
}

#[test]
fn a_later_item_an_unknown_item_and_a_closed_item_are_refused() {
    let mut later = work("w-00000001");
    later.later_at = Some(5);
    later.due = Some(Due {
        at: 10,
        timezone: "UTC".into(),
    });
    let mut done = work("w-00000002");
    done.state = WorkState::Done;
    let s = Store::new(vec![later, done]);
    let top = vec![
        row("work", "w-00000001"),
        row("work", "w-00000002"),
        row("work", "w-0000000f"),
    ];
    let reasons = s.reasons(&plan_file(top, vec![]));
    assert_eq!(reasons.len(), 3, "{reasons:?}");
    assert!(reasons[0].starts_with("top[0].work was moved down with Later"));
    assert!(reasons[1].starts_with("top[1].work is not an open Work item"));
    assert!(reasons[2].starts_with("top[2].work is not an open Work item"));
    let items = vec![row("item", "i-00000002"), row("item", "i-0000000f")];
    let reasons = s.reasons(&plan_file(items, vec![]));
    assert_eq!(reasons.len(), 2, "{reasons:?}");
    assert!(
        reasons
            .iter()
            .all(|r| r.contains("is not an open Inbox item"))
    );
}

#[test]
fn proposals_must_name_an_unsorted_capture_and_an_askable_workspace() {
    let s = Store::new(vec![work("w-00000001")]);
    let proposals = vec![
        json!({"kind": "sort", "capture": "c-00000002", "action": "note"}),
        json!({"kind": "sort", "capture": "c-00000001", "action": "work",
               "workspace": "nowhere"}),
        json!({"kind": "delegate", "work": "w-00000001", "workspace": "closed",
               "scope": "draft", "purpose": "Draft it.", "reason": "Due."}),
        json!({"kind": "delegate", "work": "w-00000001", "workspace": "Overall",
               "scope": "draft", "purpose": "Draft it.", "reason": "Due."}),
        json!({"kind": "delegate", "work": "w-0000000f", "workspace": "work",
               "scope": "draft", "purpose": "Draft it.", "reason": "Due."}),
    ];
    let reasons = s.reasons(&plan_file(vec![], proposals));
    assert_eq!(
        reasons,
        [
            "proposals[0].capture is not an unsorted Capture.",
            "proposals[1].workspace is not a workspace on this host.",
            "proposals[2].workspace is not a workspace you may ask (see the brief).",
            "proposals[3].workspace is not a workspace you may ask (see the brief).",
            "proposals[4].work is not an open Work item.",
        ]
    );
}

struct Host {
    base: PathBuf,
    config: Config,
    run_dir: PathBuf,
}

impl Drop for Host {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.base);
    }
}

fn host() -> Host {
    let base = std::env::temp_dir().join(format!(
        "muxa-plan-check-{}-{:08x}",
        std::process::id(),
        getrandom::u32().unwrap()
    ));
    let root = base.join("root");
    let run_dir = root.join("assistant/plan-t-00000a");
    crate::assistant::record::prepare(&run_dir).unwrap();
    std::fs::create_dir_all(base.join("state")).unwrap();
    let file = base.join("workspaces.toml");
    std::fs::write(
        &file,
        format!(
            "[assistant]\nenabled = true\n[[workspace]]\nname = \"Overall\"\nroot = \"{}\"\n\
             [workspace.assistant]\noverall = true\n",
            root.display()
        ),
    )
    .unwrap();
    Host {
        config: Config {
            state_dir: Some(base.join("state")),
            workspaces: Some(file),
            home: None,
        },
        base,
        run_dir,
    }
}

impl Host {
    fn plans(&self) -> Shelf<Plan> {
        Shelf::open(self.config.state_dir.as_deref().unwrap()).unwrap()
    }

    fn work(&self, item: WorkItem) -> WorkItem {
        Shelf::<WorkItem>::open(self.config.state_dir.as_deref().unwrap())
            .unwrap()
            .create::<StateError>(|id| Ok(WorkItem { id, ..item }))
            .unwrap()
    }

    fn plan(&self) -> Plan {
        self.plans()
            .create::<StateError>(|id| {
                Ok(Plan {
                    id,
                    rev: 1,
                    at: 1,
                    inputs_at: 1,
                    delivery: Delivery::Processing,
                    delivery_at: 1,
                    minutes: None,
                    run: Some("t-00000a".into()),
                    workspaces: Vec::new(),
                    top: Vec::new(),
                    due_soon: Vec::new(),
                    unanswered: Vec::new(),
                    stale: Vec::new(),
                    changed: Vec::new(),
                    user_order: Vec::new(),
                    dismissed: Vec::new(),
                    notes: None,
                    reason: None,
                })
            })
            .unwrap()
    }

    fn record(&self, plan: &str) -> Record {
        let mut r: Record = serde_json::from_value(json!({
            "version": 1, "id": "t-00000a", "rev": 1, "workspace": "Overall",
            "title": "plan", "goal": "g", "status": "open", "createdAt": 1, "updatedAt": 1,
        }))
        .unwrap();
        r.dir = Some(self.run_dir.clone());
        r.extra = RunFor::Plan(PlanRun {
            plan: plan.into(),
            notes_base: base_of(b""),
        })
        .fields();
        r
    }

    fn check(&self, record: &Record, v: &Value, attempt: u32) -> i32 {
        std::fs::write(self.run_dir.join(file::PLAN), v.to_string()).unwrap();
        match crate::assistant::check::run(&self.config, record, attempt) {
            CheckEnd::Exited { code, .. } => code,
            CheckEnd::TimedOut { .. } => panic!("an in-process check never times out"),
        }
    }

    fn log(&self, attempt: u32) -> String {
        std::fs::read_to_string(crate::loops::check::log_path(&self.run_dir, attempt)).unwrap()
    }
}

#[test]
fn a_passing_plan_is_stored_once_with_the_hosts_fields() {
    let h = host();
    let now = now();
    let soon = h.work(WorkItem {
        later_at: Some(5),
        due: Some(Due {
            at: now + 3600,
            timezone: "UTC".into(),
        }),
        ..work("w-00000000")
    });
    let later = h.work(WorkItem {
        later_at: Some(6),
        due: Some(Due {
            at: now + 3 * 86_400,
            timezone: "UTC".into(),
        }),
        ..work("w-00000000")
    });
    let open = h.work(work("w-00000000"));
    let plan = h.plan();
    let record = h.record(&plan.id);
    let mut v = plan_file(
        vec![row("work", &soon.id)],
        vec![
            json!({"kind": "delegate", "work": open.id, "workspace": "Overall",
                    "scope": "draft", "purpose": "Draft.", "reason": "Due."}),
        ],
    );
    v["plan"] = json!(plan.id);
    assert_eq!(h.check(&record, &v, 1), 1);
    let log = h.log(1);
    assert!(
        log.contains("top[0].work was moved down with Later"),
        "{log}"
    );
    assert!(log.contains("proposals[0].workspace is not a workspace you may ask"));
    assert!(!log.contains(MARK), "{log}");
    assert_eq!(
        h.plans().get(&plan.id).unwrap().unwrap().delivery,
        Delivery::Processing
    );

    v["top"] = json!([row("work", &open.id)]);
    v["proposals"] = json!([]);
    v["notes"] = json!(format!("# Overall\n{MARK}\n"));
    assert_eq!(h.check(&record, &v, 2), 0);
    let (stored, extra) = h.plans().get_with_extra(&plan.id).unwrap().unwrap();
    assert_eq!(stored.delivery, Delivery::Resulted);
    assert_eq!(stored.rev, 2);
    assert_eq!(stored.top[0].work.as_deref(), Some(open.id.as_str()));
    assert_eq!(
        stored.due_soon,
        vec![soon.id.clone()],
        "{later:?} is not due soon"
    );
    assert_eq!(stored.notes.as_ref().unwrap().base, base_of(b""));
    assert_eq!(extra[PROPOSALS_FIELD], json!([]));
    // A second pass changes nothing.
    assert_eq!(h.check(&record, &v, 3), 0);
    assert_eq!(h.plans().get(&plan.id).unwrap().unwrap().rev, 2);
}

#[test]
fn a_plan_another_run_answers_or_that_was_replaced_is_not_stored() {
    let h = host();
    let open = h.work(work("w-00000000"));
    let plan = h.plan();
    let mut v = plan_file(vec![row("work", &open.id)], vec![]);
    v["plan"] = json!(plan.id);
    let mut other = h.record(&plan.id);
    other.id = "t-00000b".into();
    assert_eq!(h.check(&other, &v, 1), 1);
    assert!(h.log(1).contains("has another run"), "{}", h.log(1));
    end(
        &h.plans(),
        &plan.id,
        None,
        Delivery::Cancelled,
        "replaced",
        9,
    )
    .unwrap();
    assert_eq!(h.check(&h.record(&plan.id), &v, 2), 1);
    assert!(h.log(2).contains("replaced"), "{}", h.log(2));
}

#[test]
fn a_stopped_loop_fails_the_plan_unless_a_user_stopped_it() {
    let h = host();
    let plan = h.plan();
    let record = h.record(&plan.id);
    let lp = |reason| {
        let mut lp = Loop::start(
            crate::records::LoopSpec {
                check: crate::assistant::record::CHECK.into(),
                max_attempts: Some(3),
                timeout: None,
            },
            None,
            false,
            1,
        );
        lp.state = LoopState::Stopped;
        lp.reason = Some(reason);
        lp
    };
    let state = h.config.state_dir.clone().unwrap();
    stopped(&state, &record, &lp(Reason::User));
    assert_eq!(
        h.plans().get(&plan.id).unwrap().unwrap().delivery,
        Delivery::Processing
    );
    stopped(&state, &record, &lp(Reason::Limit));
    let p = h.plans().get(&plan.id).unwrap().unwrap();
    assert_eq!(p.delivery, Delivery::Failed);
    assert_eq!(p.reason.as_deref(), Some("No valid plan."));
}

#[test]
fn a_plans_hypotheses_are_stored_only_when_the_brief_backs_them() {
    use crate::assistant::corrections::{self, Seen};
    use muxa_protocol::{CorrectionKind, HypothesisState};
    let h = host();
    let open = h.work(work("w-00000000"));
    let plan = h.plan();
    let seen = Seen::row(CorrectionKind::Dismissed, &open.id, None);
    let x = corrections::record(&h.config, &plan, seen, now()).unwrap();
    let carried = Carried {
        corrections: vec![x.id.clone()],
        slots: 3,
        ..Carried::default()
    };
    h.plans()
        .update_with_extra::<_, StateError>(&plan.id, |_, extra| {
            carried.put(extra);
            Ok(())
        })
        .unwrap();
    let record = h.record(&plan.id);
    let mut v = plan_file(vec![row("work", &open.id)], vec![]);
    v["plan"] = json!(plan.id);
    v["hypotheses"] = json!([{"text": format!("Not on Fridays. {MARK}"), "scope": "all",
                              "evidence": ["x-0000000f"], "confidence": "low"}]);
    assert_eq!(h.check(&record, &v, 1), 1);
    let log = h.log(1);
    assert!(
        log.contains("hypotheses[0].evidence[0] is not a correction"),
        "{log}"
    );
    assert!(!log.contains(MARK), "{log}");

    v["hypotheses"][0]["evidence"] = json!([x.id]);
    assert_eq!(h.check(&record, &v, 2), 0);
    let stored = crate::assistant::shelf::<muxa_protocol::Hypothesis>(&h.config)
        .unwrap()
        .all()
        .unwrap();
    assert_eq!(stored.len(), 1, "{stored:?}");
    assert_eq!(stored[0].state, HypothesisState::Proposed);
    assert_eq!(stored[0].plan, plan.id);
    assert_eq!(stored[0].evidence, [x.id]);
}
