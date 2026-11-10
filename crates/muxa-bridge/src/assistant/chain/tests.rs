use std::path::PathBuf;

use muxa_protocol::{
    AssistResult, ErrorCode, Handoff, ItemKind, ItemState, RequestStep, StepBy, WorkItem,
};
use serde_json::json;

use super::next::{AUTO_FIELD, Via};
use super::*;
use crate::assistant::record::{RequestRun, RunFor};
use crate::assistant::steps;
use crate::items::store::ItemStore;
use crate::records::Record;

const LEAD: &str = "lead";
const STRATEGIST: &str = "strategist";
const ARCHITECT: &str = "architect";
const RUN: &str = "t-0000a1";

struct Host {
    base: PathBuf,
    config: Config,
}

impl Drop for Host {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.base);
    }
}

/// Workspace `lisk` with a staff of three; `extra` goes into its
/// `[workspace.assistant]`, `host` into `[assistant]`.
fn host(extra: &str, host: &str) -> Host {
    let base = std::env::temp_dir().join(format!(
        "muxa-chain-{}-{:08x}",
        std::process::id(),
        getrandom::u32().unwrap()
    ));
    std::fs::create_dir_all(base.join("state")).unwrap();
    std::fs::create_dir_all(base.join("lisk")).unwrap();
    let base = base.canonicalize().unwrap();
    let worker = |name: &str| {
        format!("[[worker]]\nname = \"{name}\"\ndescription = \"Job\"\nagent = \"claude-code\"\n")
    };
    std::fs::write(
        base.join("workspaces.toml"),
        format!(
            "[assistant]\nenabled = true\n{host}\n\
             [[workspace]]\nname = \"lisk\"\nroot = \"{}\"\n[workspace.assistant]\n\
             delegate = true\nstaff = [\"{LEAD}\", \"{STRATEGIST}\", \"{ARCHITECT}\"]\n{extra}\n\
             {}{}{}",
            base.join("lisk").display(),
            worker(LEAD),
            worker(STRATEGIST),
            worker(ARCHITECT),
        ),
    )
    .unwrap();
    let config = Config {
        state_dir: Some(base.join("state")),
        workspaces: Some(base.join("workspaces.toml")),
        home: None,
    };
    Host { base, config }
}

fn step(n: u32, worker: &str, run: &str, handoff: Option<&str>) -> RequestStep {
    RequestStep {
        run: Some(run.into()),
        delivery: Delivery::Resulted,
        handoff: handoff.map(|to| Handoff {
            worker: to.into(),
            purpose: "Compare them.".into(),
        }),
        ..steps::fresh(
            n,
            worker,
            if n == 1 {
                StepBy::Lead
            } else {
                StepBy::Handoff
            },
            1,
        )
    }
}

fn result(version: u64, step: u32) -> AssistResult {
    AssistResult {
        writes: Vec::new(),
        version,
        at: 1,
        summary: "Two options.".into(),
        stale: false,
        findings: Vec::new(),
        open_questions: Vec::new(),
        next: Vec::new(),
        files: Vec::new(),
        possible_duplicates: Vec::new(),
        notes: None,
        worker: None,
        step: Some(step),
    }
}

/// Version 1 whose step 1 (the lead) handed the request to the strategist.
fn waiting_request() -> AssistantRequest {
    serde_json::from_value(json!({
        "id": "q-0000000a", "work": "w-0000000b", "version": 1, "workspace": "lisk",
        "scope": "research", "delivery": "resulted", "deliveryAt": 1,
        "createdAt": 1, "updatedAt": 1, "run": RUN,
        "results": [result(1, 1)],
        "steps": [step(1, LEAD, RUN, Some(STRATEGIST))],
    }))
    .unwrap()
}

impl Host {
    fn save(&self, q: AssistantRequest) -> String {
        let work = crate::assistant::shelf::<WorkItem>(&self.config)
            .unwrap()
            .create::<StateError>(|id| {
                Ok(serde_json::from_value(json!({
                    "id": id, "rev": 1, "title": "Roadmap", "state": "open",
                    "createdAt": 1, "updatedAt": 1, "workspace": "lisk",
                }))
                .unwrap())
            })
            .unwrap();
        crate::assistant::shelf::<AssistantRequest>(&self.config)
            .unwrap()
            .create::<StateError>(|id| {
                Ok(AssistantRequest {
                    id,
                    work: work.id,
                    ..q
                })
            })
            .unwrap()
            .id
    }

    fn get(&self, id: &str) -> (AssistantRequest, Extra) {
        crate::assistant::shelf::<AssistantRequest>(&self.config)
            .unwrap()
            .get_with_extra(id)
            .unwrap()
            .unwrap()
    }

    fn items(&self) -> Vec<muxa_protocol::InboxItem> {
        ItemStore::open(self.config.state_dir.as_ref().unwrap())
            .unwrap()
            .get(RUN)
            .unwrap()
            .items
            .into_iter()
            .map(|s| s.item)
            .collect()
    }

    /// The run of step `step`, as `create()` leaves it.
    fn record(&self, id: &str, request: &str, step: u32) {
        let mut r: Record = serde_json::from_value(json!({
            "version": 1, "id": id, "rev": 1, "workspace": "lisk", "title": "t",
            "goal": format!("Brief.\n\n{}", steps::marker(request, 1, step)),
            "status": "open", "createdAt": crate::records::now(), "updatedAt": 1,
        }))
        .unwrap();
        r.extra.extend(
            RunFor::Request(RequestRun {
                request: request.into(),
                version: 1,
                work: "w-0000000b".into(),
                notes_base: None,
                step: Some(step),
                member: None,
                shared_base: None,
            })
            .fields(),
        );
        let path = self.base.join(format!("state/{id}.json"));
        std::fs::write(path, serde_json::to_vec(&r).unwrap()).unwrap();
    }

    fn claim(&self, id: &str, via: Via, at: u64) -> Result<bool, muxa_protocol::ErrorBody> {
        next::claim(&self.config, "claude", id, 1, 1, via, at)
            .map(|c| matches!(c, next::Claimed::Start(..)))
    }
}

#[test]
fn the_tick_asks_starts_or_resolves_by_the_chain_and_the_mode() {
    let q = waiting_request();
    let none = Extra::new();
    assert_eq!(
        pending(&q).map(|(s, h)| (s, h.worker.as_str())),
        Some((1, STRATEGIST))
    );
    assert_eq!(
        todo(&q, &none, Some(Handoffs::Review)),
        Some(Todo::Ask { step: 1 })
    );
    assert_eq!(
        todo(&q, &none, Some(Handoffs::Auto)),
        Some(Todo::Start { step: 1 })
    );
    assert_eq!(todo(&q, &none, Some(Handoffs::None)), None);

    // Asked once: nothing more until the chain moves.
    let mut asked = Extra::new();
    let mark = Waiting {
        version: 1,
        step: 1,
        task: Some(RUN.into()),
        ..Waiting::default()
    };
    set_waiting(&mut asked, &mark);
    assert_eq!(todo(&q, &asked, Some(Handoffs::Review)), None);

    // A new version, a cancel, or an ended chain resolves the waiting item once.
    let v2 = AssistantRequest {
        version: 2,
        delivery: Delivery::Pending,
        steps: vec![steps::fresh(1, LEAD, StepBy::Lead, 2)],
        ..q.clone()
    };
    let resolve = Some(Todo::Resolve {
        task: RUN.into(),
        why: "A new version of the request ended this chain.",
    });
    assert_eq!(todo(&v2, &asked, Some(Handoffs::Review)), resolve);
    let mut ended = Extra::new();
    set_waiting(
        &mut ended,
        &Waiting {
            ended: true,
            ..mark.clone()
        },
    );
    assert!(matches!(
        todo(&q, &ended, Some(Handoffs::Review)),
        Some(Todo::Resolve { .. })
    ));
    let mut resolved = Extra::new();
    set_waiting(
        &mut resolved,
        &Waiting {
            ended: true,
            resolved: true,
            ..mark
        },
    );
    assert_eq!(todo(&q, &resolved, Some(Handoffs::Review)), None);

    // A running step, or one without a hand-off, waits on nothing.
    let running = AssistantRequest {
        delivery: Delivery::Processing,
        ..q.clone()
    };
    assert_eq!(pending(&running), None);
    let mut answered = q.clone();
    answered.steps[0].handoff = None;
    assert_eq!(todo(&answered, &none, Some(Handoffs::Auto)), None);
}

#[test]
fn one_item_per_step_names_both_workers_and_resolves_once() {
    let h = host("", "");
    let id = h.save(waiting_request());
    let (q, _) = h.get(&id);
    let made = items::make(&h.config, RUN, &q, 1, None).unwrap();
    assert!(made.is_some());
    assert_eq!(items::make(&h.config, RUN, &q, 1, None).unwrap(), None);
    let [item] = h.items().try_into().unwrap();
    assert_eq!(
        (item.kind, item.state),
        (ItemKind::Handoff, ItemState::Open)
    );
    assert_eq!(item.question, "lead asks strategist to: Compare them.");
    assert_eq!(
        (item.workspace.as_str(), item.title.as_str()),
        ("lisk", "Roadmap")
    );
    let p = item.handoff.unwrap();
    assert_eq!((p.request.as_str(), p.version, p.step), (id.as_str(), 1, 1));
    assert_eq!((p.from.as_str(), p.worker.as_str()), (LEAD, STRATEGIST));
    assert_eq!(items::resolve(&h.config, RUN, "gone").unwrap(), 1);
    assert_eq!(items::resolve(&h.config, RUN, "gone").unwrap(), 0);
    assert_eq!(h.items()[0].state, ItemState::Resolved);
}

#[test]
fn a_claim_adds_one_step_and_a_crash_finds_its_run_by_the_marker() {
    let h = host("", "");
    let id = h.save(waiting_request());
    let now = crate::records::now();
    assert!(h.claim(&id, Via::Approved, now).unwrap());
    let (q, extra) = h.get(&id);
    let last = q.steps.last().unwrap();
    assert_eq!(
        (last.step, last.worker.as_str(), last.by, q.delivery),
        (2, STRATEGIST, StepBy::Handoff, Delivery::Pending)
    );
    assert_eq!(steps::creating(&extra, 1, 2), Some(now));
    // Approved twice while the run is made: no second step.
    let busy = h.claim(&id, Via::Approved, now).unwrap_err();
    assert_eq!(busy.code, ErrorCode::Conflict);
    // A crash between the claim and the run's link: the marker finds it.
    h.record("t-0000a2", &id, 2);
    let later = now + crate::assistant::ask::CREATING_LIMIT + 1;
    assert!(!h.claim(&id, Via::Approved, later).unwrap());
    let (q, _) = h.get(&id);
    assert_eq!((q.steps.len(), q.run.as_deref()), (2, Some("t-0000a2")));
}

#[test]
fn a_new_version_a_cancel_or_max_steps_refuses_the_next_step() {
    let h = host("max_steps = 2", "");
    let id = h.save(waiting_request());
    let work = h.get(&id).0.work;
    assert_eq!(end(&h.config, &id, 1).unwrap(), Some(work));
    assert_eq!(end(&h.config, &id, 2).unwrap(), None);
    let e = h.claim(&id, Via::Approved, 10).unwrap_err();
    assert!(e.message.contains("cancelled"), "{}", e.message);

    let mut q = waiting_request();
    q.version = 2;
    q.results = vec![result(2, 1)];
    let moved = h.save(q);
    let e = h.claim(&moved, Via::Approved, 10).unwrap_err();
    assert_eq!(e.code, ErrorCode::Conflict);

    let mut q = waiting_request();
    q.steps
        .push(step(2, STRATEGIST, "t-0000a3", Some(ARCHITECT)));
    q.results.insert(0, result(1, 2));
    let last = h.save(q);
    let e = next::claim(&h.config, "claude", &last, 1, 2, Via::Approved, 10)
        .err()
        .unwrap();
    assert!(e.message.contains("max_steps"), "{}", e.message);
}

#[test]
fn auto_steps_stop_at_the_hosts_daily_limit() {
    let h = host(
        "handoffs = \"auto\"\nmax_open_runs = 3",
        "max_auto_handoffs_per_day = 1",
    );
    let first = h.save(waiting_request());
    let now = crate::records::now();
    assert!(h.claim(&first, Via::Auto, now).unwrap());
    assert_eq!(h.get(&first).1[AUTO_FIELD], json!([now]));
    let second = h.save(waiting_request());
    let e = h.claim(&second, Via::Auto, now).unwrap_err();
    assert_eq!(e.code, ErrorCode::Conflict);
    assert!(
        e.message.contains("max_auto_handoffs_per_day"),
        "{}",
        e.message
    );
    // The user's approval is not an auto step.
    assert!(h.claim(&second, Via::Approved, now).unwrap());
}
