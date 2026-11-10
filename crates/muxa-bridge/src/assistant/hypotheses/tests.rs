use muxa_protocol::{CorrectionKind, Delivery};

use super::*;
use crate::assistant::corrections::tests::{NOW, host};
use crate::assistant::corrections::{self, Seen};
use crate::assistant::policies::{self, Fields};

fn new(text: &str, evidence: &str) -> NewHypothesis {
    NewHypothesis {
        text: text.into(),
        scope: "work".into(),
        evidence: vec![evidence.into()],
        confidence: Confidence::Medium,
    }
}

#[test]
fn texts_compare_without_case_spacing_or_a_closing_period() {
    assert_eq!(
        normalized("  Mornings  are for\tWriting. "),
        normalized("mornings are for writing")
    );
    assert_ne!(normalized("Mornings"), normalized("Evenings"));
    assert!(is_id("h-0a1b2c3d", "h-"));
    assert!(!is_id("h-0A1B2C3D", "h-"));
    assert!(!is_id("x-0a1b2c3d", "h-"));
}

#[test]
fn a_plans_hypotheses_are_stored_proposed_for_14_days_once_per_text() {
    let h = host();
    let now = crate::records::now();
    let plan = h.plan(NOW, Delivery::Resulted, &[]);
    let seen = Seen::row(CorrectionKind::Dismissed, "w-00000001", Some("work".into()));
    let x = corrections::record(&h.config, &plan, seen, now).unwrap();
    let fields = Fields {
        text: Some("Recruiting comes first".into()),
        scope: Some("all".into()),
        from: None,
        until: None,
    };
    policies::add(&h.config, fields, now).unwrap();
    let made = adopt(
        &h.config,
        &plan.id,
        vec![
            new("Mornings are for writing.", &x.id),
            new("mornings are for  writing", &x.id),
            new("Recruiting comes first.", &x.id),
            new("Forgotten ground.", "x-0000000f"),
        ],
        now,
    );
    assert_eq!(made.len(), 1, "{made:?}");
    let stored = &made[0];
    assert_eq!(stored.state, HypothesisState::Proposed);
    assert_eq!(stored.expires_at, now + PROPOSED_FOR);
    assert_eq!((stored.plan.as_str(), stored.at), (plan.id.as_str(), now));
    assert_eq!(stored.evidence, std::slice::from_ref(&x.id));
    // A later plan's same guess makes no second record.
    assert!(
        adopt(
            &h.config,
            &plan.id,
            vec![new("MORNINGS are for writing", &x.id)],
            now
        )
        .is_empty()
    );
    assert_eq!(list(&h.config, now).unwrap(), made);
    assert!(list(&h.config, now + PROPOSED_FOR).unwrap().is_empty());
}

#[test]
fn no_more_than_ten_are_proposed_at_once() {
    let h = host();
    let now = crate::records::now();
    let plan = h.plan(NOW, Delivery::Resulted, &[]);
    let seen = Seen::row(CorrectionKind::Pinned, "w-00000001", None);
    let x = corrections::record(&h.config, &plan, seen, now).unwrap();
    let guesses: Vec<NewHypothesis> = (0..12).map(|n| new(&format!("Guess {n}"), &x.id)).collect();
    let made = adopt(&h.config, &plan.id, guesses, now);
    assert_eq!(made.len(), MAX_PROPOSED);
    let all = super::super::shelf::<Hypothesis>(&h.config)
        .unwrap()
        .all()
        .unwrap();
    assert_eq!(slots(&all, now), 0);
    assert_eq!(slots(&all, now + PROPOSED_FOR), MAX_PER_PLAN);
}
