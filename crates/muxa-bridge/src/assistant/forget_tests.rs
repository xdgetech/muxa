use muxa_protocol::{Confidence, CorrectionKind, Delivery, ItemState};
use serde_json::json;

use super::*;
use crate::assistant::corrections::{self, Seen};
use crate::assistant::hypotheses::answer::tests::{Fixture, REASON, RUN, TEXT, fixture};
use crate::assistant::hypotheses::answer::{confirm, reject};
use crate::assistant::hypotheses::{self, NewHypothesis, PROPOSED_FOR};
use crate::assistant::notes::base_of;
use crate::assistant::plan::cite::{self, Carried, Told};
use crate::assistant::record::PlanRun;

fn ids(f: &Fixture) -> Vec<String> {
    let mut all: Vec<String> = crate::assistant::shelf::<Hypothesis>(f.config())
        .unwrap()
        .all()
        .unwrap()
        .into_iter()
        .map(|h| h.id)
        .collect();
    all.sort();
    all
}

fn keyed(f: &Fixture) -> Vec<String> {
    f.items()
        .into_iter()
        .filter_map(|i| i.hypothesis.map(|h| h.hypothesis))
        .collect()
}

#[test]
fn a_forgotten_correction_takes_the_proposed_hypotheses_it_grounds_with_it() {
    let f = fixture();
    let x1 = f.correction(None);
    let x2 = f.correction(None);
    let grounded = f.propose(&["Grounded on x1.", "Rejected, on x1."], &x1.id);
    let other = f.propose(&["Grounded on x2."], &x2.id).remove(0);
    hypotheses::items::publish(f.config(), f.now);
    let rejected = &grounded[1];
    reject(f.config(), RUN, &f.item_of(rejected), None, f.now).unwrap();

    forget(f.config(), &x1.id, f.now).unwrap();
    let kept = corrections::live(f.config(), f.now).unwrap();
    assert!(kept.iter().all(|c| c.id != x1.id), "{kept:?}");
    let mut left = vec![rejected.id.clone(), other.id.clone()];
    left.sort();
    assert_eq!(ids(&f), left);
    assert!(!keyed(&f).contains(&grounded[0].id));
    assert!(keyed(&f).contains(&other.id));
    // Forgotten already.
    let e = forget(f.config(), &x1.id, f.now).unwrap_err();
    assert_eq!(e.code, ErrorCode::BadRequest);
}

#[test]
fn a_hypothesis_is_forgotten_with_its_item_unless_it_became_a_policy() {
    let f = fixture();
    let x = f.correction(None);
    let made = f.propose(&["Proposed.", "Rejected.", "Expired.", "Confirmed."], &x.id);
    hypotheses::items::publish(f.config(), f.now);
    reject(f.config(), RUN, &f.item_of(&made[1]), None, f.now).unwrap();
    let (policy, _) = confirm(f.config(), RUN, &f.item_of(&made[3]), f.now).unwrap();
    crate::assistant::shelf::<Hypothesis>(f.config())
        .unwrap()
        .update::<_, crate::routines::store::StateError>(&made[2].id, |h| {
            h.state = HypothesisState::Expired;
            Ok(())
        })
        .unwrap();
    for h in &made[..3] {
        forget(f.config(), &h.id, f.now).unwrap();
        assert!(f.stored(&h.id).is_none());
        assert!(!keyed(&f).contains(&h.id), "{} kept its item", h.id);
    }
    let e = forget(f.config(), &made[3].id, f.now).unwrap_err();
    assert_eq!(e.code, ErrorCode::BadRequest);
    assert!(e.message.contains(&policy.id), "{}", e.message);
    assert!(f.stored(&made[3].id).is_some());
    for record in ["h-0000000f", "y-00000001", "x-ZZ", ""] {
        let e = forget(f.config(), record, f.now).unwrap_err();
        assert_eq!(e.code, ErrorCode::BadRequest, "{record}");
    }
}

/// Sentinels: a forgotten reason and hypothesis text never reach the next
/// brief, and a plan briefed before the forget cannot cite them.
#[test]
fn the_next_brief_lacks_forgotten_records_and_a_plan_citing_them_is_refused() {
    let f = fixture();
    let seen = Seen {
        reason: Some(REASON.into()),
        ..Seen::row(CorrectionKind::Dismissed, "w-00000001", None)
    };
    let x = corrections::record(f.config(), &f.plan, seen, f.now).unwrap();
    let ground = f.correction(None);
    let new = NewHypothesis {
        text: format!("Writing comes first. {TEXT}"),
        scope: "all".into(),
        evidence: vec![ground.id.clone()],
        confidence: Confidence::Medium,
    };
    let h = hypotheses::adopt(f.config(), &f.plan.id, vec![new], f.now).remove(0);
    let before = brief(&f, "before");
    assert!(before.0.contains(REASON) && before.0.contains(TEXT));
    assert!(before.1.corrections.contains(&x.id) && before.1.hypotheses.contains(&h.id));

    forget(f.config(), &x.id, f.now).unwrap();
    forget(f.config(), &h.id, f.now).unwrap();
    let (after, carried) = brief(&f, "after");
    for mark in [REASON, TEXT] {
        assert!(!after.contains(mark), "the brief holds {mark}:\n{after}");
    }
    assert!(!carried.corrections.contains(&x.id) && carried.hypotheses.is_empty());

    // A plan of the old brief cites both.
    let v = json!({"version": 1, "plan": "p-0a1b2c3d",
        "top": [{"work": "w-00000001", "why": "Why.", "next": "Next.",
                 "minutes": {"low": 1, "estimated": 2, "high": 3},
                 "basis": [{"kind": "correction", "text": "You moved it.", "source": x.id},
                           {"kind": "hypothesis", "text": "My guess.", "source": h.id}]}],
        "hypotheses": [{"text": "Fridays are short.", "scope": "all",
                        "evidence": [x.id], "confidence": "low"}]});
    let run = PlanRun {
        plan: "p-0a1b2c3d".into(),
        notes_base: base_of(b""),
    };
    let checked = crate::assistant::plan::file::validate(v.to_string().as_bytes(), &run).unwrap();
    let live = Live::read(&f);
    let told = Told {
        carried: &before.1,
        policies: &[],
        hypotheses: &live.hypotheses,
        corrections: &live.corrections,
        now: f.now,
    };
    let workspaces = f.config().load_workspaces().unwrap();
    let reasons = cite::reasons(&checked, &told, &workspaces);
    let fields: Vec<&str> = reasons
        .iter()
        .map(|r| r.split(' ').next().unwrap_or_default())
        .collect();
    assert_eq!(
        fields,
        [
            "top[0].basis[0].source",
            "top[0].basis[1].source",
            "hypotheses[0].evidence[0]"
        ],
        "{reasons:?}"
    );
}

struct Live {
    hypotheses: Vec<Hypothesis>,
    corrections: Vec<Correction>,
}

impl Live {
    fn read(f: &Fixture) -> Self {
        Live {
            hypotheses: crate::assistant::shelf::<Hypothesis>(f.config())
                .unwrap()
                .all()
                .unwrap(),
            corrections: corrections::live(f.config(), f.now).unwrap(),
        }
    }
}

/// The brief of a new pending plan, and what it carried.
fn brief(f: &Fixture, name: &str) -> (String, Carried) {
    let pending = f.h.plan(f.now, Delivery::Pending, &[]);
    let dir = f.config().state_dir.as_ref().unwrap().join(name);
    std::fs::create_dir_all(dir.join(".muxa/assistant")).unwrap();
    crate::assistant::plan::brief::write(f.config(), &pending.id, &dir).unwrap();
    let text = std::fs::read_to_string(dir.join(crate::assistant::plan::brief::BRIEF)).unwrap();
    let (_, extra) = crate::assistant::plan::shelf(f.config())
        .unwrap()
        .get_with_extra(&pending.id)
        .unwrap()
        .unwrap();
    (text, Carried::of(&extra))
}

#[test]
fn items_of_a_forgotten_hypothesis_go_whatever_their_state() {
    let f = fixture();
    let x = f.correction(None);
    let h = f.propose(&[TEXT], &x.id).remove(0);
    hypotheses::items::publish(f.config(), f.now);
    hypotheses::tick(f.config(), f.now + PROPOSED_FOR);
    assert_eq!(f.item_of(&h).state, ItemState::Resolved);
    forget(f.config(), &h.id, f.now).unwrap();
    assert!(f.items().is_empty());
}
