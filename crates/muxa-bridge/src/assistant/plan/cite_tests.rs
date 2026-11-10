use std::path::Path;

use muxa_protocol::{Confidence, HypothesisState};
use serde_json::{Value, json};

use super::*;
use crate::assistant::notes::base_of;
use crate::assistant::record::PlanRun;

/// A hypothesis text: never in a reason.
const MARK: &str = "hypothesis-mark-5d31";
const NOW: u64 = 1_762_740_000;

fn workspaces() -> Workspaces {
    let root = std::env::temp_dir();
    let root = root.to_string_lossy();
    let text = format!(
        "[assistant]\nenabled = true\n\
         [[workspace]]\nname = \"Overall\"\nroot = \"{root}\"\n[workspace.assistant]\noverall = true\n\
         [[workspace]]\nname = \"work\"\nroot = \"{root}\"\n[workspace.assistant]\ndelegate = true\n\
         [[workspace]]\nname = \"home\"\nroot = \"{root}\"\n[workspace.assistant]\ndelegate = true\n"
    );
    Workspaces::parse(&text, Path::new("w.toml"), None).unwrap()
}

fn correction(id: &str, kind: CorrectionKind, workspace: Option<&str>) -> Correction {
    Correction {
        id: id.into(),
        kind,
        at: NOW - 60,
        plan: "p-00000001".into(),
        row: Some("w-00000001".into()),
        in_top: true,
        workspace: workspace.map(Into::into),
        before: Vec::new(),
        after: Vec::new(),
        reason: None,
    }
}

fn policy(id: &str, until: Option<u64>) -> Policy {
    Policy {
        id: id.into(),
        rev: 1,
        text: "Recruiting first.".into(),
        scope: "all".into(),
        from: None,
        until,
        source: "user".into(),
        created_at: 1,
        updated_at: 1,
    }
}

fn hypothesis(id: &str, state: HypothesisState) -> Hypothesis {
    Hypothesis {
        id: id.into(),
        text: "Mornings are for writing.".into(),
        scope: "all".into(),
        evidence: vec!["x-00000002".into()],
        confidence: Confidence::Low,
        state,
        plan: "p-00000001".into(),
        expires_at: NOW + 86_400,
        at: NOW - 86_400,
    }
}

struct Store {
    carried: Carried,
    policies: Vec<Policy>,
    hypotheses: Vec<Hypothesis>,
    corrections: Vec<Correction>,
}

/// x1 deferred in work, x2 and x3 dismissed in work and home, x4 dismissed
/// on nobody's item, x5 deferred in the overall workspace; x6 is live but
/// the brief left it out. y1 is active, y2 expired; h1 proposed, h2 rejected.
fn store() -> Store {
    use CorrectionKind::{Deferred, Dismissed};
    let ids = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
    Store {
        carried: Carried {
            policies: ids(&["y-00000001", "y-00000002"]),
            hypotheses: ids(&["h-00000001"]),
            corrections: ids(&[
                "x-00000001",
                "x-00000002",
                "x-00000003",
                "x-00000004",
                "x-00000005",
            ]),
            slots: 3,
        },
        policies: vec![policy("y-00000001", None), policy("y-00000002", Some(5))],
        hypotheses: vec![
            hypothesis("h-00000001", HypothesisState::Proposed),
            hypothesis("h-00000002", HypothesisState::Rejected),
        ],
        corrections: vec![
            correction("x-00000001", Deferred, Some("work")),
            correction("x-00000002", Dismissed, Some("work")),
            correction("x-00000003", Dismissed, Some("home")),
            correction("x-00000004", Dismissed, None),
            correction("x-00000005", Deferred, Some("Overall")),
            correction("x-00000006", Dismissed, Some("home")),
        ],
    }
}

impl Store {
    fn reasons(&self, v: &Value) -> Vec<String> {
        let run = PlanRun {
            plan: "p-0a1b2c3d".into(),
            notes_base: base_of(b""),
        };
        let checked = super::super::file::validate(v.to_string().as_bytes(), &run).unwrap();
        let told = Told {
            carried: &self.carried,
            policies: &self.policies,
            hypotheses: &self.hypotheses,
            corrections: &self.corrections,
            now: NOW,
        };
        let reasons = reasons(&checked, &told, &workspaces());
        for r in &reasons {
            assert!(!r.contains(MARK), "a reason quotes the plan: {r}");
        }
        reasons
    }
}

fn guess(scope: &str, evidence: &[&str]) -> Value {
    json!({"text": format!("The user writes in the morning. {MARK}"), "scope": scope,
           "evidence": evidence, "confidence": "low"})
}

fn plan(basis: Vec<Value>, hypotheses: Vec<Value>) -> Value {
    json!({"version": 1, "plan": "p-0a1b2c3d",
           "top": [{"work": "w-00000001", "why": "Why.", "next": "Next.",
                    "minutes": {"low": 5, "estimated": 10, "high": 20}, "basis": basis}],
           "hypotheses": hypotheses})
}

fn cite(kind: &str, source: &str) -> Value {
    json!({"kind": kind, "text": "Shaped the row.", "source": source})
}

#[test]
fn citations_and_hypotheses_the_brief_backs_pass() {
    let s = store();
    let basis = vec![
        cite("policy", "y-00000001"),
        cite("hypothesis", "h-00000001"),
        cite("correction", "x-00000002"),
    ];
    let hypotheses = vec![
        guess("work", &["x-00000001", "x-00000002"]),
        guess("all", &["x-00000002", "x-00000003"]),
        guess("all", &["x-00000002", "x-00000004"]),
    ];
    assert_eq!(s.reasons(&plan(basis, hypotheses)), Vec::<String>::new());
    // The overall workspace's rows are the user's own, too.
    let own = vec![guess("all", &["x-00000002", "x-00000005"])];
    assert_eq!(s.reasons(&plan(vec![], own)), Vec::<String>::new());
}

#[test]
fn later_alone_is_no_ground() {
    let s = store();
    let reasons = s.reasons(&plan(vec![], vec![guess("work", &["x-00000001"])]));
    assert_eq!(
        reasons,
        [
            "hypotheses[0].evidence is deferred corrections only. Later means not now: it is no \
          ground for a hypothesis."
        ]
    );
    let two = vec![guess("all", &["x-00000001", "x-00000005"])];
    assert_eq!(s.reasons(&plan(vec![], two)).len(), 1);
}

#[test]
fn scope_all_from_one_workspaces_rows_is_refused() {
    let s = store();
    let reasons = s.reasons(&plan(
        vec![],
        vec![guess("all", &["x-00000001", "x-00000002"])],
    ));
    assert_eq!(reasons.len(), 1, "{reasons:?}");
    assert!(reasons[0].starts_with("hypotheses[0]: scope all needs evidence beyond one"));
    let scoped = s.reasons(&plan(vec![], vec![guess("work", &["x-00000002"])]));
    assert!(scoped.is_empty(), "{scoped:?}");
    let yours = s.reasons(&plan(vec![], vec![guess("Overall", &["x-00000002"])]));
    assert_eq!(
        yours,
        ["hypotheses[0].scope must be all or a workspace on this host other than yours."]
    );
}

#[test]
fn ids_the_brief_did_not_carry_or_that_are_gone_are_refused() {
    let s = store();
    let basis = vec![
        cite("policy", "y-00000002"),
        cite("policy", "y-0000000f"),
        cite("hypothesis", "h-00000002"),
        cite("correction", "x-00000006"),
    ];
    let hypotheses = vec![guess("home", &["x-00000003", "x-00000006", "x-0000000f"])];
    let reasons = s.reasons(&plan(basis, hypotheses));
    let first = |r: &String| r.split(' ').next().unwrap_or_default().to_owned();
    assert_eq!(
        reasons.iter().map(first).collect::<Vec<_>>(),
        [
            "top[0].basis[0].source",
            "top[0].basis[1].source",
            "top[0].basis[2].source",
            "top[0].basis[3].source",
            "hypotheses[0].evidence[1]",
            "hypotheses[0].evidence[2]",
        ],
        "{reasons:?}"
    );
    // Forgotten after the brief: carried, but no longer in the store.
    let mut forgot = store();
    forgot.corrections.retain(|c| c.id != "x-00000002");
    forgot.hypotheses.clear();
    let basis = vec![
        cite("hypothesis", "h-00000001"),
        cite("correction", "x-00000002"),
    ];
    let reasons = forgot.reasons(&plan(basis, vec![guess("work", &["x-00000002"])]));
    assert_eq!(reasons.len(), 3, "{reasons:?}");
}

#[test]
fn more_hypotheses_than_slots_are_refused() {
    let mut s = store();
    s.carried.slots = 1;
    let two = vec![
        guess("work", &["x-00000002"]),
        guess("home", &["x-00000003"]),
    ];
    assert_eq!(
        s.reasons(&plan(vec![], two.clone())),
        ["hypotheses has 2 entries; this plan may propose 1."]
    );
    // Ten open since the brief: none left.
    s.carried.slots = 3;
    s.hypotheses = (0..10)
        .map(|n| hypothesis(&format!("h-1000000{n}"), HypothesisState::Proposed))
        .collect();
    assert_eq!(
        s.reasons(&plan(vec![], two)),
        ["hypotheses has 2 entries; this plan may propose 0."]
    );
    // A plan briefed before ADR 0050 may propose none and cite nothing.
    let none = Told::nothing();
    let run = PlanRun {
        plan: "p-0a1b2c3d".into(),
        notes_base: base_of(b""),
    };
    let v = plan(
        vec![cite("policy", "y-00000001")],
        vec![guess("work", &["x-00000002"])],
    );
    let checked = super::super::file::validate(v.to_string().as_bytes(), &run).unwrap();
    assert_eq!(reasons(&checked, &none, &workspaces()).len(), 3);
}

#[test]
fn what_a_brief_carried_round_trips_through_the_plan_file() {
    let s = store();
    let mut extra = Extra::new();
    assert_eq!(Carried::of(&extra), Carried::default());
    s.carried.put(&mut extra);
    assert_eq!(Carried::of(&extra), s.carried);
}
