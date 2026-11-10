use std::path::PathBuf;
use std::sync::Arc;

use muxa_protocol::{
    AssistScope, AssistantRequest, Capture, CaptureState, DelegateProposal, Delivery, ErrorCode,
    InboxAction, InboxItem, ItemKind, ItemState, Plan, SortAction, SortProposal, WorkItem,
    WorkState,
};
use serde_json::json;

use super::approve;
use super::inbox::{self, PUBLISHED_FIELD};
use super::*;
use crate::agents::Registry;
use crate::assistant::capture::{NewCapture, save};
use crate::assistant::plan::check::PROPOSALS_FIELD;
use crate::assistant::plan::file::Proposal;
use crate::assistant::record::{RequestRun, RunFor};
use crate::assistant::store::{Extra, Shelf};
use crate::items::answer::{self, Answer};
use crate::items::store::ItemStore;
use crate::records::Record;
use crate::routines::store::StateError;
use crate::task_records::{ClientFuture, ClientSource, Config, Pending};

/// A sentinel in purposes and reasons; `tests/assistant_plans.rs` checks the log for it.
const MARK: &str = "purpose-mark-6c1f";
/// The plan run whose item file holds the proposals.
const RUN: &str = "t-00000a";

struct NoTmux;

impl ClientSource for NoTmux {
    fn client(&self) -> ClientFuture<'_> {
        Box::pin(async { Err(muxa_tmux::Error::Exited("no tmux in this test".into())) })
    }
}

struct Host {
    base: PathBuf,
    cx: Context,
}

impl Drop for Host {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.base);
    }
}

/// `overall` plans; `w` and `x` delegate with the given `overall_requests`.
/// `extra` goes into `[assistant]`, `w_extra` into `w`'s table.
fn host(w: &str, x: &str, extra: &str, w_extra: &str) -> Host {
    let base = std::env::temp_dir().join(format!(
        "muxa-proposals-{}-{:08x}",
        std::process::id(),
        getrandom::u32().unwrap()
    ));
    for d in ["state", "overall", "w", "x"] {
        std::fs::create_dir_all(base.join(d)).unwrap();
    }
    let base = base.canonicalize().unwrap();
    let table = |name: &str, mode: &str, more: &str| {
        format!(
            "[[workspace]]\nname = \"{name}\"\nroot = \"{}\"\n[workspace.assistant]\n\
             delegate = true\noverall_requests = \"{mode}\"\n{more}\n",
            base.join(name).display()
        )
    };
    std::fs::write(
        base.join("workspaces.toml"),
        format!(
            "[assistant]\nenabled = true\n{extra}\n[[workspace]]\nname = \"overall\"\n\
             root = \"{}\"\n[workspace.assistant]\noverall = true\nplans = []\n{}{}",
            base.join("overall").display(),
            table("w", w, w_extra),
            table("x", x, ""),
        ),
    )
    .unwrap();
    let cx = Context {
        tmux: Arc::new(NoTmux),
        tasks: Arc::new(crate::tasks::Config {
            launcher: None,
            command: "claude".into(),
            tmux: "tmux".into(),
            socket: None,
        }),
        records: Arc::new(Config {
            state_dir: Some(base.join("state")),
            workspaces: Some(base.join("workspaces.toml")),
            home: None,
        }),
        agents: Arc::new(Registry::new(Vec::new())),
        pending: Pending::default(),
        daemon: true,
    };
    Host { base, cx }
}

impl Host {
    fn config(&self) -> &Config {
        &self.cx.records
    }

    fn capture(&self, text: &str) -> Capture {
        let new = NewCapture {
            client_id: format!("client-{:08x}", getrandom::u32().unwrap()),
            text: text.into(),
            source: None,
            workspace: Some("w".into()),
            assist: None,
        };
        save(self.config(), new).unwrap().0
    }

    fn work(&self, title: &str) -> WorkItem {
        crate::assistant::shelf::<WorkItem>(self.config())
            .unwrap()
            .create::<StateError>(|id| {
                Ok(WorkItem {
                    id,
                    rev: 1,
                    title: title.into(),
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

    /// A stored plan of `RUN` with `proposals`, at `inputs_at`.
    fn plan(&self, inputs_at: u64, proposals: &[Proposal]) -> Plan {
        Shelf::<Plan>::open(self.config().state_dir.as_deref().unwrap())
            .unwrap()
            .create_with_extra::<StateError>(|id| {
                let plan = Plan {
                    id,
                    rev: 2,
                    at: inputs_at,
                    inputs_at,
                    delivery: Delivery::Resulted,
                    delivery_at: inputs_at,
                    minutes: None,
                    run: Some(RUN.into()),
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
                };
                let mut extra = Extra::new();
                extra.insert(
                    PROPOSALS_FIELD.into(),
                    serde_json::to_value(proposals).unwrap(),
                );
                extra.insert(
                    crate::assistant::plan::WORKSPACE_FIELD.into(),
                    json!("overall"),
                );
                Ok((plan, extra))
            })
            .unwrap()
    }

    fn items(&self) -> Vec<InboxItem> {
        let store = ItemStore::open(self.config().state_dir.as_deref().unwrap()).unwrap();
        store
            .get(RUN)
            .unwrap()
            .items
            .into_iter()
            .map(|s| crate::items::shown(&s.item))
            .collect()
    }

    fn item(&self, kind: ItemKind) -> InboxItem {
        self.items().into_iter().find(|i| i.kind == kind).unwrap()
    }

    fn requests(&self) -> Vec<AssistantRequest> {
        crate::assistant::shelf::<AssistantRequest>(self.config())
            .unwrap()
            .all()
            .unwrap()
    }

    fn work_count(&self) -> usize {
        crate::assistant::shelf::<WorkItem>(self.config())
            .unwrap()
            .all()
            .unwrap()
            .len()
    }

    async fn approve(&self, item: &str) -> (Vec<Message>, Option<ErrorBody>) {
        answer(&self.cx, 7, item).await.expect("a proposal item")
    }

    /// An open run of `w`, as `create()` leaves it.
    fn open_run(&self) {
        let mut r: Record = serde_json::from_value(json!({
            "version": 1, "id": "t-0000c1", "rev": 1, "workspace": "w", "title": "Run",
            "goal": "Brief.", "status": "open", "createdAt": crate::records::now(),
            "updatedAt": 1,
        }))
        .unwrap();
        r.extra.extend(
            RunFor::Request(RequestRun {
                request: "q-0000000f".into(),
                version: 1,
                work: "w-00000000".into(),
                notes_base: None,
                step: None,
                member: None,
                shared_base: None,
            })
            .fields(),
        );
        let path = self.base.join("state/t-0000c1.json");
        std::fs::write(path, serde_json::to_vec(&r).unwrap()).unwrap();
    }
}

fn sort_of(c: &Capture) -> Proposal {
    Proposal::Sort(SortProposal {
        capture: c.id.clone(),
        action: SortAction::Work,
        workspace: Some("w".into()),
        title: Some("Try the tool".into()),
        work: None,
    })
}

fn delegate_to(w: &WorkItem, workspace: &str) -> Proposal {
    Proposal::Delegate(DelegateProposal {
        worker: None,
        work: w.id.clone(),
        workspace: workspace.into(),
        scope: AssistScope::Compare,
        purpose: format!("Compare the offers. {MARK}"),
        reason: format!("Due Friday. {MARK}"),
    })
}

fn saved_kinds(messages: &[Message]) -> Vec<&'static str> {
    messages
        .iter()
        .map(|m| match m {
            Message::CaptureSaved { .. } => "capture-saved",
            Message::WorkSaved { .. } => "work-saved",
            Message::InboxSaved { .. } => "inbox-saved",
            _ => "other",
        })
        .collect()
}

#[test]
fn a_plans_proposals_become_items_once() {
    let h = host("review", "review", "", "");
    let c = h.capture("This might work for business B");
    let w = h.work("Compare vendor offers");
    let plan = h.plan(10, &[sort_of(&c), delegate_to(&w, "w")]);
    let autos = inbox::publish(h.config());
    assert!(autos.is_empty(), "review is never sent by the host");
    let items = h.items();
    assert_eq!(
        items.iter().map(|i| i.kind).collect::<Vec<_>>(),
        [ItemKind::Sort, ItemKind::Delegate]
    );
    assert_eq!(items[0].question, "Make a Work item in w: Try the tool");
    assert_eq!(items[1].question, "Ask w to compare: Compare vendor offers");
    for i in &items {
        assert_eq!(i.workspace, "overall");
        assert_eq!(i.state, ItemState::Open);
        use InboxAction::*;
        assert_eq!(i.answers, [Approve, Reject, Later]);
    }
    // Published once: a second scan makes nothing, and the plan keeps its rev.
    inbox::publish(h.config());
    assert_eq!(h.items().len(), 2);
    let (p, extra) = Shelf::<Plan>::open(h.config().state_dir.as_deref().unwrap())
        .unwrap()
        .get_with_extra(&plan.id)
        .unwrap()
        .unwrap();
    assert_eq!((p.rev, extra.get(PUBLISHED_FIELD)), (2, Some(&json!(true))));
}

#[test]
fn only_the_newest_plan_asks_and_an_open_item_is_not_asked_twice() {
    let h = host("review", "review", "", "");
    let w = h.work("Compare vendor offers");
    h.plan(10, &[delegate_to(&w, "w")]);
    inbox::publish(h.config());
    let newer = h.plan(20, &[delegate_to(&w, "w")]);
    let older = h.plan(5, &[delegate_to(&w, "x")]);
    inbox::publish(h.config());
    assert_eq!(h.items().len(), 1, "{:?}", h.items());
    let plans = Shelf::<Plan>::open(h.config().state_dir.as_deref().unwrap()).unwrap();
    for id in [&newer.id, &older.id] {
        let (_, extra) = plans.get_with_extra(id).unwrap().unwrap();
        assert!(extra.contains_key(PUBLISHED_FIELD));
    }
}

#[tokio::test]
async fn approving_a_sort_twice_sorts_once() {
    let h = host("review", "review", "", "");
    let c = h.capture("This might work for business B");
    h.plan(10, &[sort_of(&c)]);
    inbox::publish(h.config());
    let item = h.item(ItemKind::Sort);
    let (first, error) = h.approve(&item.id).await;
    assert_eq!(error, None);
    assert_eq!(saved_kinds(&first), ["capture-saved", "inbox-saved"]);
    let Message::CaptureSaved {
        entry,
        work: Some(made),
        ..
    } = &first[0]
    else {
        panic!("{first:?}")
    };
    assert_eq!(entry.state, CaptureState::Sorted);
    assert_eq!(made.title, "Try the tool");
    assert_eq!(made.workspace.as_deref(), Some("w"));
    let answered = h.item(ItemKind::Sort);
    assert_eq!(
        (answered.state, answered.answer),
        (ItemState::Answered, Some(InboxAction::Approve))
    );
    let (second, error) = h.approve(&item.id).await;
    assert_eq!(error, None);
    let Message::CaptureSaved {
        entry: again,
        work: Some(same),
        ..
    } = &second[0]
    else {
        panic!("{second:?}")
    };
    assert_eq!((&again.id, &same.id), (&entry.id, &made.id));
    assert_eq!(h.work_count(), 1);
}

#[tokio::test]
async fn approving_a_delegate_twice_asks_once() {
    let h = host("review", "review", "", "");
    let w = h.work("Compare vendor offers");
    h.plan(10, &[delegate_to(&w, "w")]);
    inbox::publish(h.config());
    let item = h.item(ItemKind::Delegate);
    let (first, error) = h.approve(&item.id).await;
    // No tmux here: the request is made and its run does not start.
    assert_eq!(error.map(|e| e.code), Some(ErrorCode::Tmux));
    assert_eq!(saved_kinds(&first), ["work-saved", "inbox-saved"]);
    let Message::WorkSaved {
        request: Some(q), ..
    } = &first[0]
    else {
        panic!("{first:?}")
    };
    assert_eq!(
        (q.workspace.as_str(), q.scope, q.version),
        ("w", AssistScope::Compare, 1)
    );
    assert!(q.purpose.as_deref().is_some_and(|p| p.contains(MARK)));
    let (second, error) = h.approve(&item.id).await;
    assert_eq!(error, None);
    let Message::WorkSaved {
        request: Some(again),
        ..
    } = &second[0]
    else {
        panic!("{second:?}")
    };
    assert_eq!((&again.id, again.version), (&q.id, 1));
    assert_eq!(h.requests().len(), 1);
}

#[tokio::test]
async fn reject_answers_the_item_and_makes_nothing() {
    let h = host("review", "review", "", "");
    let w = h.work("Compare vendor offers");
    h.plan(10, &[delegate_to(&w, "w")]);
    inbox::publish(h.config());
    let item = h.item(ItemKind::Delegate);
    let reject = |text: String| Answer {
        item: item.id.clone(),
        action: InboxAction::Reject,
        text: Some(text),
        option: None,
        worktree: None,
    };
    let oversize = answer::update(&h.cx, reject("x".repeat(501))).await;
    assert_eq!(oversize.unwrap_err().code, ErrorCode::BadRequest);
    let reason = format!("Not this week. {MARK}");
    let rejected = answer::update(&h.cx, reject(reason.clone())).await.unwrap();
    assert_eq!(
        (rejected.state, rejected.answer),
        (ItemState::Answered, Some(InboxAction::Reject))
    );
    assert!(h.requests().is_empty());
    // ADR 0050 §2: one correction, with the reason.
    let now = crate::records::now();
    let recorded = crate::assistant::corrections::live(h.config(), now).unwrap();
    assert_eq!(recorded.len(), 1);
    assert_eq!(recorded[0].row.as_deref(), Some(item.id.as_str()));
    assert_eq!(recorded[0].reason.as_deref(), Some(reason.as_str()));
    let (_, error) = h.approve(&item.id).await;
    assert_eq!(error.map(|e| e.code), Some(ErrorCode::BadRequest));
    assert!(h.requests().is_empty());
}

#[tokio::test]
async fn a_workspace_with_none_refuses_and_the_item_stays_open() {
    let h = host("none", "review", "", "");
    let w = h.work("Compare vendor offers");
    h.plan(10, &[delegate_to(&w, "w")]);
    inbox::publish(h.config());
    let item = h.item(ItemKind::Delegate);
    let (messages, error) = h.approve(&item.id).await;
    let error = error.unwrap();
    assert_eq!(error.code, ErrorCode::Unsupported);
    assert!(
        error.message.contains("overall_requests"),
        "{}",
        error.message
    );
    assert!(messages.is_empty());
    assert_eq!(h.item(ItemKind::Delegate).state, ItemState::Open);
    assert!(h.requests().is_empty());
}

#[tokio::test]
async fn a_capture_sorted_meanwhile_closes_the_item_with_a_reason() {
    let h = host("review", "review", "", "");
    let c = h.capture("This might work for business B");
    h.plan(10, &[sort_of(&c)]);
    inbox::publish(h.config());
    let item = h.item(ItemKind::Sort);
    crate::assistant::work::sort(
        h.config(),
        crate::assistant::work::Sort {
            capture: c.id.clone(),
            rev: c.rev,
            action: SortAction::Note,
            work: None,
            title: None,
            workspace: None,
        },
    )
    .unwrap();
    let (messages, error) = h.approve(&item.id).await;
    let error = error.unwrap();
    assert_eq!(error.code, ErrorCode::Conflict);
    assert!(
        error.message.contains("was sorted before"),
        "{}",
        error.message
    );
    assert_eq!(saved_kinds(&messages), ["inbox-saved"]);
    let closed = h.item(ItemKind::Sort);
    assert_eq!(closed.state, ItemState::Resolved);
    assert!(closed.answers.is_empty());
    assert_eq!(h.work_count(), 0);
}

#[tokio::test]
async fn auto_sends_once_and_keeps_the_answered_item() {
    let h = host("auto", "review", "", "");
    let c = h.capture("This might work for business B");
    let w = h.work("Compare vendor offers");
    h.plan(10, &[sort_of(&c), delegate_to(&w, "w")]);
    tick(&h.cx).await;
    tick(&h.cx).await;
    assert_eq!(h.requests().len(), 1);
    assert_eq!(h.requests()[0].version, 1);
    let sent = h.item(ItemKind::Delegate);
    assert_eq!(
        (sent.state, sent.answer),
        (ItemState::Answered, Some(InboxAction::Approve))
    );
    assert!(
        sent.summary
            .as_deref()
            .unwrap()
            .contains("Sent without review")
    );
    assert_eq!(
        h.item(ItemKind::Sort).state,
        ItemState::Open,
        "sorting is reviewed"
    );
    // The user's approve returns what the host sent.
    let (again, _) = h.approve(&sent.id).await;
    assert_eq!(saved_kinds(&again), ["work-saved", "inbox-saved"]);
    assert_eq!(h.requests().len(), 1);
}

#[tokio::test]
async fn auto_past_a_budget_waits_for_review_with_the_reason() {
    let h = host("auto", "review", "", "max_open_runs = 1");
    h.open_run();
    let w = h.work("Compare vendor offers");
    h.plan(10, &[delegate_to(&w, "w")]);
    tick(&h.cx).await;
    tick(&h.cx).await;
    assert!(h.requests().is_empty());
    let held = h.item(ItemKind::Delegate);
    assert_eq!(held.state, ItemState::Open);
    let summary = held.summary.unwrap();
    assert!(summary.starts_with("Not sent automatically:"), "{summary}");
    assert!(summary.contains("max_open_runs"), "{summary}");
}

#[tokio::test]
async fn auto_stops_at_max_auto_requests_per_day() {
    let h = host("auto", "auto", "max_auto_requests_per_day = 1", "");
    let w = h.work("Compare vendor offers");
    h.plan(10, &[delegate_to(&w, "w"), delegate_to(&w, "x")]);
    tick(&h.cx).await;
    tick(&h.cx).await;
    let requests = h.requests();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].workspace, "w");
    let held: Vec<InboxItem> = h
        .items()
        .into_iter()
        .filter(|i| i.state == ItemState::Open)
        .collect();
    assert_eq!(held.len(), 1);
    assert_eq!(held[0].delegate.as_ref().unwrap().workspace, "x");
    let summary = held[0].summary.as_deref().unwrap();
    assert!(summary.contains("max_auto_requests_per_day"), "{summary}");
    // The user may still send it.
    let (messages, _) = h.approve(&held[0].id).await;
    assert_eq!(saved_kinds(&messages), ["work-saved", "inbox-saved"]);
    assert_eq!(h.requests().len(), 2);
}

#[tokio::test]
async fn a_host_approval_of_a_review_workspace_is_not_made() {
    let h = host("review", "review", "", "");
    let w = h.work("Compare vendor offers");
    h.plan(10, &[delegate_to(&w, "w")]);
    tick(&h.cx).await;
    assert!(h.requests().is_empty());
    let found = approve::find(h.config(), &h.item(ItemKind::Delegate).id)
        .unwrap()
        .unwrap();
    let a = inbox::Auto {
        item: found.item.id.clone(),
        workspace: "w".into(),
    };
    assert!(
        inbox::may_send(h.config(), &a, crate::records::now())
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn approve_asks_the_staff_member_the_plan_named() {
    let worker = |name: &str| {
        format!("[[worker]]\nname = \"{name}\"\ndescription = \"Job\"\nagent = \"claude-code\"\n")
    };
    let staff = format!(
        "staff = [\"w-lead\", \"w-research\"]\n{}{}",
        worker("w-lead"),
        worker("w-research")
    );
    let h = host("review", "review", "", &staff);
    let w = h.work("Compare vendor offers");
    let mut named = delegate_to(&w, "w");
    if let Proposal::Delegate(d) = &mut named {
        d.worker = Some("w-research".into());
    }
    h.plan(10, &[named]);
    inbox::publish(h.config());
    let item = h.item(ItemKind::Delegate);
    assert!(
        item.question.starts_with("Ask w-research in w to compare:"),
        "{}",
        item.question
    );
    let (first, _) = h.approve(&item.id).await;
    let Message::WorkSaved {
        request: Some(q), ..
    } = &first[0]
    else {
        panic!("{first:?}")
    };
    let step = &q.steps[0];
    assert_eq!(
        (step.worker.as_str(), step.by),
        ("w-research", muxa_protocol::StepBy::Overall)
    );
}
