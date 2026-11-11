use std::path::PathBuf;

use muxa_protocol::{AssistScope, InboxAction, WorkState};
use serde_json::json;

use super::*;
use crate::assistant::record::{PlanRun, RequestRun, RunFor};
use crate::items::store::TaskItems;
use crate::records::{Loop, LoopSpec};
use crate::task_records::handoff::ASKS_TRUST_FIELD;

const NOW: u64 = 1_000_000;
const RUN: &str = "t-00000a";

struct Host {
    base: PathBuf,
    config: Config,
}

impl Drop for Host {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.base);
    }
}

fn host() -> Host {
    let base = std::env::temp_dir().join(format!(
        "muxa-assistant-failures-{}-{:08x}",
        std::process::id(),
        getrandom::u32().unwrap()
    ));
    std::fs::create_dir_all(base.join("state")).unwrap();
    std::fs::create_dir_all(base.join("root/assistant/hosting-t-00000a")).unwrap();
    std::fs::write(
        base.join("workspaces.toml"),
        format!(
            "[[workspace]]\nname = \"w\"\nroot = \"{}\"\n",
            base.join("root").display()
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

impl Host {
    fn state(&self) -> &Path {
        self.config.state_dir.as_deref().unwrap()
    }

    fn work(&self) -> WorkItem {
        Shelf::<WorkItem>::open(self.state())
            .unwrap()
            .create::<StateError>(|id| {
                Ok(WorkItem {
                    id,
                    rev: 1,
                    title: "Pick a host".into(),
                    state: WorkState::Open,
                    created_at: 1,
                    updated_at: 1,
                    workspace: Some("w".into()),
                    later_at: None,
                    merged_into: None,
                    pinned_at: None,
                    due: None,
                    captures: Vec::new(),
                    requests: Vec::new(),
                    tasks: Vec::new(),
                })
            })
            .unwrap()
    }

    fn request(&self, work: &str, delivery: Delivery, at: u64) -> AssistantRequest {
        Shelf::<AssistantRequest>::open(self.state())
            .unwrap()
            .create::<StateError>(|id| {
                Ok(AssistantRequest {
                    steps: Vec::new(),
                    id,
                    work: work.into(),
                    version: 2,
                    workspace: "w".into(),
                    scope: AssistScope::Research,
                    delivery,
                    delivery_at: at,
                    created_at: 1,
                    updated_at: at,
                    purpose: None,
                    run: Some(RUN.into()),
                    results: Vec::new(),
                })
            })
            .unwrap()
    }

    fn plan(&self, reason: Option<&str>) -> Plan {
        Shelf::<Plan>::open(self.state())
            .unwrap()
            .create::<StateError>(|id| {
                Ok(serde_json::from_value(json!({
                    "id": id, "rev": 1, "at": NOW, "inputsAt": NOW,
                    "delivery": "failed", "deliveryAt": NOW, "run": RUN,
                    "reason": reason
                }))
                .unwrap())
            })
            .unwrap()
    }

    /// The run's record, stopped by `stop` and with the trust mark when `trust`.
    fn run(&self, run: RunFor, stop: Option<Reason>, trust: bool) {
        let mut r: Record = serde_json::from_value(json!({
            "version": 1, "id": RUN, "rev": 1, "workspace": "w", "title": "hosting",
            "goal": "g", "status": "open", "createdAt": 1, "updatedAt": 1,
            "dir": self.base.join("root/assistant/hosting-t-00000a"),
        }))
        .unwrap();
        r.extra = run.fields();
        if let Some(reason) = stop {
            let spec = LoopSpec {
                check: "true".into(),
                max_attempts: Some(3),
                timeout: Some(60),
            };
            let lp = Loop::start(spec, None, false, 10).stopped(reason, 20);
            r.set_loop(Some(&lp)).unwrap();
        }
        if trust {
            r.extra.insert(ASKS_TRUST_FIELD.into(), 20.into());
        }
        std::fs::write(
            self.state().join(format!("{RUN}.json")),
            serde_json::to_vec(&r).unwrap(),
        )
        .unwrap();
    }

    fn items(&self) -> TaskItems {
        ItemStore::open(self.state()).unwrap().get(RUN).unwrap()
    }

    fn root(&self) -> PathBuf {
        self.base.join("root").canonicalize().unwrap()
    }
}

fn request_run(q: &AssistantRequest) -> RunFor {
    RunFor::Request(RequestRun {
        request: q.id.clone(),
        version: q.version,
        work: q.work.clone(),
        notes_base: None,
        step: None,
        member: None,
        shared_base: None,
    })
}

fn plan_run(p: &Plan) -> RunFor {
    RunFor::Plan(PlanRun {
        plan: p.id.clone(),
        notes_base: "0".repeat(64),
    })
}

#[test]
fn a_failed_request_makes_one_failure_item_with_the_trust_reason() {
    let h = host();
    let work = h.work();
    let q = h.request(&work.id, Delivery::Failed, NOW);
    h.run(request_run(&q), Some(Reason::NotSent), true);

    scan(&h.config, NOW);
    scan(&h.config, NOW + 5);
    let items = h.items();
    assert_eq!(items.items.len(), 1, "{items:?}");
    let stored = &items.items[0];
    assert_eq!(stored.key, request_key(&q.id, 2, 1));
    let item = &stored.item;
    assert_eq!(
        (item.kind, item.state),
        (ItemKind::Failure, ItemState::Open)
    );
    assert_eq!(
        item.question,
        format!(
            "Claude Code has not trusted {}. Open claude there once and accept.",
            h.root().display()
        )
    );
    assert_eq!(
        (item.workspace.as_str(), item.title.as_str()),
        ("w", "Pick a host")
    );
    assert_eq!(
        (item.task.as_deref(), item.session.as_deref()),
        (None, None)
    );
    assert_eq!(
        item.answers,
        [
            InboxAction::Dismiss,
            InboxAction::AskAgent,
            InboxAction::Later
        ]
    );
}

#[test]
fn without_the_dialog_the_reason_is_the_runs_own() {
    let h = host();
    let work = h.work();
    let q = h.request(&work.id, Delivery::Failed, NOW);
    h.run(request_run(&q), Some(Reason::NotSent), false);
    scan(&h.config, NOW);
    assert_eq!(h.items().items[0].item.question, NOT_SENT);

    let mut r: Record =
        serde_json::from_slice(&std::fs::read(h.state().join(format!("{RUN}.json"))).unwrap())
            .unwrap();
    let lp = r.task_loop().unwrap().unwrap();
    r.set_loop(Some(&Loop {
        reason: Some(Reason::Limit),
        ..lp
    }))
    .unwrap();
    assert_eq!(
        from_record(&r),
        "The agent's result did not pass the check, also after the fail prompts."
    );
    r.extra.remove("loop");
    r.status = Status::Dropped;
    r.note = Some("The run did not start: no tmux".into());
    assert_eq!(from_record(&r), "The run did not start: no tmux");
    r.note = Some("Version 3 replaced this run.".into());
    assert_eq!(from_record(&r), NO_RESULT);
}

#[test]
fn an_old_failure_or_an_unfailed_request_makes_no_item() {
    let h = host();
    let work = h.work();
    let q = h.request(&work.id, Delivery::Failed, NOW - WINDOW - 1);
    h.run(request_run(&q), Some(Reason::NotSent), false);
    scan(&h.config, NOW);
    assert!(h.items().items.is_empty(), "made before this host did");

    let h = host();
    let work = h.work();
    let q = h.request(&work.id, Delivery::Processing, NOW);
    h.run(request_run(&q), None, false);
    scan(&h.config, NOW);
    assert!(h.items().items.is_empty());
}

#[test]
fn a_failed_plan_makes_one_item_with_its_reason_and_the_dialog_first() {
    let h = host();
    let p = h.plan(Some("The run did not start: no tmux"));
    h.run(plan_run(&p), None, false);
    scan(&h.config, NOW);
    scan(&h.config, NOW);
    let items = h.items();
    assert_eq!(items.items.len(), 1);
    assert_eq!(items.items[0].key, plan_key(&p.id));
    let item = &items.items[0].item;
    assert_eq!(item.question, "The run did not start: no tmux");
    assert_eq!(
        (item.title.as_str(), item.kind),
        (PLAN_TITLE, ItemKind::Failure)
    );

    let h = host();
    let p = h.plan(None);
    h.run(plan_run(&p), Some(Reason::NotSent), true);
    scan(&h.config, NOW);
    assert!(
        h.items().items[0]
            .item
            .question
            .starts_with("Claude Code has not trusted ")
    );
}

#[test]
fn a_long_reason_is_cut_in_the_question_and_kept_whole_in_the_summary() {
    let f = Failure {
        task: RUN.into(),
        key: "failure:k".into(),
        workspace: "w".into(),
        title: "t".into(),
        reason: format!("Claude Code has not trusted /{}.", "d/".repeat(150)),
    };
    let item = item_for(&f, "i-00000001".into(), NOW);
    assert_eq!(item.question.chars().count(), MAX_QUESTION);
    assert_eq!(item.summary.as_deref(), Some(f.reason.as_str()));
    let short = Failure {
        reason: NOT_SENT.into(),
        ..f
    };
    assert_eq!(item_for(&short, "i-00000002".into(), NOW).summary, None);
}
