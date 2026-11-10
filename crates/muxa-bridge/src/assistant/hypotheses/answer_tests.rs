use std::sync::Arc;

use muxa_protocol::{Confidence, CorrectionKind, Delivery, Plan, Request};

use super::*;
use crate::agents::Registry;
use crate::assistant::corrections::tests::{Host, NOW, host};
use crate::assistant::corrections::{self as corrs, Seen};
use crate::assistant::hypotheses::{self, NewHypothesis, PROPOSED_FOR, items};
use crate::assistant::policies::{Fields, MAX_ACTIVE};
use crate::task_records::{ClientFuture, ClientSource, Pending};

/// The plan run whose item file holds the hypothesis items.
pub(crate) const RUN: &str = "t-00000a";
/// A hypothesis text and a reason: never in a log or an error.
pub(crate) const TEXT: &str = "hypothesis-mark-0f3c";
pub(crate) const REASON: &str = "reason-mark-a8e1";

pub(crate) struct Fixture {
    pub h: Host,
    pub plan: Plan,
    pub now: u64,
}

/// A resulted plan of the run [`RUN`], with one correction as ground.
pub(crate) fn fixture() -> Fixture {
    let h = host();
    let plan = h.plan(NOW, Delivery::Resulted, &[]);
    crate::assistant::plan::shelf(&h.config)
        .unwrap()
        .update::<_, StateError>(&plan.id, |p| {
            p.run = Some(RUN.into());
            Ok(())
        })
        .unwrap();
    Fixture {
        h,
        plan,
        now: crate::records::now(),
    }
}

impl Fixture {
    pub fn config(&self) -> &Config {
        &self.h.config
    }

    pub fn correction(&self, reason: Option<&str>) -> muxa_protocol::Correction {
        let seen = Seen {
            reason: reason.map(Into::into),
            ..Seen::row(CorrectionKind::Dismissed, "w-00000001", Some("work".into()))
        };
        corrs::record(self.config(), &self.plan, seen, self.now).unwrap()
    }

    /// Proposed hypotheses of the plan, one per text, on `evidence`.
    pub fn propose(&self, texts: &[&str], evidence: &str) -> Vec<Hypothesis> {
        let new = texts
            .iter()
            .map(|t| NewHypothesis {
                text: (*t).into(),
                scope: "work".into(),
                evidence: vec![evidence.into()],
                confidence: Confidence::Low,
            })
            .collect();
        hypotheses::adopt(self.config(), &self.plan.id, new, self.now)
    }

    /// The items of [`RUN`]'s file.
    pub fn items(&self) -> Vec<InboxItem> {
        let dir = self.config().state_dir.as_ref().unwrap();
        ItemStore::open(dir)
            .unwrap()
            .get(RUN)
            .unwrap()
            .items
            .into_iter()
            .map(|s| s.item)
            .collect()
    }

    pub fn item_of(&self, h: &Hypothesis) -> InboxItem {
        self.items()
            .into_iter()
            .find(|i| i.hypothesis.as_ref().is_some_and(|p| p.hypothesis == h.id))
            .unwrap_or_else(|| panic!("no item for {}", h.id))
    }

    pub fn stored(&self, id: &str) -> Option<Hypothesis> {
        crate::assistant::shelf::<Hypothesis>(self.config())
            .unwrap()
            .get(id)
            .unwrap()
    }
}

#[test]
fn the_newest_plans_hypotheses_get_one_item_each_and_a_repeated_text_none() {
    let f = fixture();
    let x = f.correction(None);
    let made = f.propose(
        &[
            &format!("Mornings are for writing. {TEXT}"),
            "No calls on Friday.",
        ],
        &x.id,
    );
    // Stored before the user wrote the same words as a policy.
    let fields = Fields {
        text: Some("no calls on friday".into()),
        scope: Some("all".into()),
        from: None,
        until: None,
    };
    policies::add(f.config(), fields, f.now).unwrap();
    items::publish(f.config(), f.now);
    items::publish(f.config(), f.now);
    let listed = f.items();
    assert_eq!(listed.len(), 1, "{listed:?}");
    let item = &listed[0];
    assert_eq!(item.kind, ItemKind::Hypothesis);
    assert_eq!(item.workspace, "secretary");
    assert_eq!(
        item.answers,
        [
            InboxAction::Confirm,
            InboxAction::Reject,
            InboxAction::Later
        ]
    );
    let p = item.hypothesis.as_ref().unwrap();
    assert_eq!((&p.hypothesis, &p.scope), (&made[0].id, &made[0].scope));
    assert_eq!(p.evidence, made[0].evidence);
}

#[test]
fn confirm_twice_makes_one_policy() {
    let f = fixture();
    let x = f.correction(None);
    let h = f.propose(&[TEXT], &x.id).remove(0);
    items::publish(f.config(), f.now);
    let item = f.item_of(&h);
    let (first, shown) = confirm(f.config(), RUN, &item, f.now).unwrap();
    assert_eq!((first.text.as_str(), first.scope.as_str()), (TEXT, "work"));
    assert_eq!(first.source, h.id);
    assert_eq!(shown.state, ItemState::Answered);
    assert_eq!(shown.answer, Some(InboxAction::Confirm));
    assert_eq!(f.stored(&h.id).unwrap().state, HypothesisState::Confirmed);
    let (again, _) = confirm(f.config(), RUN, &f.item_of(&h), f.now).unwrap();
    assert_eq!(again, first);
    assert_eq!(policies::list(f.config(), f.now).unwrap(), [first]);
    // A confirmed hypothesis is the policy now: reject names it.
    let e = reject(f.config(), RUN, &f.item_of(&h), None, f.now).unwrap_err();
    assert!(e.message.contains("update-policy"), "{}", e.message);
}

#[test]
fn confirm_at_the_policy_limit_conflicts_and_the_item_stays_open() {
    let f = fixture();
    let x = f.correction(None);
    let h = f.propose(&[TEXT], &x.id).remove(0);
    items::publish(f.config(), f.now);
    for n in 0..MAX_ACTIVE {
        let fields = Fields {
            text: Some(format!("Policy {n}")),
            scope: Some("all".into()),
            from: None,
            until: None,
        };
        policies::add(f.config(), fields, f.now).unwrap();
    }
    let e = confirm(f.config(), RUN, &f.item_of(&h), f.now).unwrap_err();
    assert_eq!(e.code, ErrorCode::Conflict);
    assert!(!e.message.contains(TEXT));
    assert_eq!(f.item_of(&h).state, ItemState::Open);
    assert_eq!(f.stored(&h.id).unwrap().state, HypothesisState::Proposed);
}

#[test]
fn reject_keeps_it_rejected_for_90_days_and_records_one_correction() {
    let f = fixture();
    let x = f.correction(None);
    let h = f.propose(&[TEXT], &x.id).remove(0);
    items::publish(f.config(), f.now);
    let item = f.item_of(&h);
    let shown = reject(f.config(), RUN, &item, Some(REASON.into()), f.now).unwrap();
    assert_eq!(shown.answer, Some(InboxAction::Reject));
    let stored = f.stored(&h.id).unwrap();
    assert_eq!(stored.state, HypothesisState::Rejected);
    assert_eq!(stored.expires_at, f.now + REJECTED_FOR);
    reject(f.config(), RUN, &f.item_of(&h), None, f.now).unwrap();
    let recorded: Vec<_> = corrs::live(f.config(), f.now)
        .unwrap()
        .into_iter()
        .filter(|c| c.kind == CorrectionKind::Rejected)
        .collect();
    assert_eq!(recorded.len(), 1, "{recorded:?}");
    assert_eq!(recorded[0].row.as_deref(), Some(item.id.as_str()));
    assert_eq!(recorded[0].reason.as_deref(), Some(REASON));
    let listed = hypotheses::list(f.config(), f.now).unwrap();
    assert_eq!(listed[0].state, HypothesisState::Rejected);
    assert!(
        hypotheses::list(f.config(), f.now + REJECTED_FOR)
            .unwrap()
            .is_empty()
    );
    // A rejected guess is no slot and cannot be confirmed.
    assert!(confirm(f.config(), RUN, &f.item_of(&h), f.now).is_err());
}

#[test]
fn an_unanswered_hypothesis_expires_after_14_days_and_its_item_resolves() {
    let f = fixture();
    let x = f.correction(None);
    let h = f.propose(&[TEXT], &x.id).remove(0);
    hypotheses::tick(f.config(), f.now);
    assert_eq!(f.item_of(&h).state, ItemState::Open);
    hypotheses::tick(f.config(), f.now + PROPOSED_FOR);
    assert_eq!(f.stored(&h.id).unwrap().state, HypothesisState::Expired);
    let item = f.item_of(&h);
    assert_eq!(item.state, ItemState::Resolved);
    assert!(item.answers.is_empty());
    hypotheses::tick(f.config(), f.now + PROPOSED_FOR + REJECTED_FOR);
    assert!(f.stored(&h.id).is_none());
}

struct NoTmux;

impl ClientSource for NoTmux {
    fn client(&self) -> ClientFuture<'_> {
        Box::pin(async { Err(muxa_tmux::Error::Exited("no tmux in this test".into())) })
    }
}

fn context(config: &Config) -> Context {
    Context {
        tmux: Arc::new(NoTmux),
        tasks: Arc::new(crate::tasks::Config {
            launcher: None,
            command: "claude".into(),
            tmux: "tmux".into(),
            socket: None,
        }),
        records: Arc::new(config.clone()),
        agents: Arc::new(Registry::new(Vec::new())),
        pending: Pending::default(),
        daemon: true,
    }
}

#[tokio::test]
async fn update_inbox_confirm_answers_with_the_policy_then_the_item() {
    let f = fixture();
    let x = f.correction(None);
    let h = f.propose(&[TEXT], &x.id).remove(0);
    items::publish(f.config(), f.now);
    let item = f.item_of(&h);
    let cx = context(f.config());
    let request = |action: InboxAction, text: Option<&str>| Request::UpdateInbox {
        id: 7,
        entry: item.id.clone(),
        action,
        until: None,
        text: text.map(Into::into),
        option: None,
        worktree: None,
    };
    let bad_reason = "x".repeat(501);
    let (_, error) =
        crate::routines::handle(&cx, request(InboxAction::Reject, Some(&bad_reason))).await;
    assert_eq!(error.map(|e| e.code), Some(ErrorCode::BadRequest));
    let (messages, error) = crate::routines::handle(&cx, request(InboxAction::Confirm, None)).await;
    assert!(error.is_none(), "{error:?}");
    match messages.as_slice() {
        [
            Message::PolicySaved { policy: 7, entry },
            Message::InboxSaved {
                inbox: 7,
                item: Some(saved),
                ..
            },
        ] => {
            assert_eq!(entry.source, h.id);
            assert_eq!(saved.answer, Some(InboxAction::Confirm));
        }
        other => panic!("{other:?}"),
    }
}
