use serde_json::{Value, json};

use super::*;
use crate::assistant::notes::base_of;

/// Text the agent wrote: never in a reason.
const MARK: &str = "plan-mark-6b1f";

fn run() -> PlanRun {
    PlanRun {
        plan: "p-0a1b2c3d".into(),
        notes_base: base_of(b""),
    }
}

fn row(work: &str) -> Value {
    json!({"work": work, "why": format!("Due soon. {MARK}"), "next": "Write it.",
           "minutes": {"low": 30, "estimated": 45, "high": 60},
           "basis": [{"kind": "due", "text": "Due Friday", "at": 1_762_930_800}]})
}

fn plan(top: Vec<Value>, proposals: Vec<Value>) -> Value {
    json!({"version": 1, "plan": "p-0a1b2c3d", "top": top, "proposals": proposals})
}

fn check(v: &Value) -> Result<Checked, Vec<String>> {
    validate(v.to_string().as_bytes(), &run())
}

fn refused(v: &Value) -> Vec<String> {
    let reasons = check(v).expect_err("refused");
    for r in &reasons {
        assert!(!r.contains(MARK), "a reason quotes the file: {r}");
    }
    reasons
}

fn delegate(workspace: &str) -> Value {
    json!({"kind": "delegate", "work": "w-0b1c2d3e", "workspace": workspace,
           "scope": "compare", "purpose": format!("Compare the offers. {MARK}"),
           "reason": "Due Friday."})
}

#[test]
fn a_good_plan_passes_with_its_rows_and_proposals() {
    let item = json!({"item": "i-1a2b3c4d", "why": "A capture waits.", "next": "Sort it.",
                      "minutes": {"low": 1, "estimated": 2, "high": 5}, "missing": []});
    let sort = json!({"kind": "sort", "capture": "c-4e5f6a7b", "action": "link",
                      "work": "w-0b1c2d3e"});
    let mut v = plan(
        vec![row("w-0b1c2d3e"), item],
        vec![sort, delegate("work"), delegate("home")],
    );
    v["notes"] = json!("# Overall\n");
    let c = check(&v).unwrap();
    assert_eq!(c.top.len(), 2);
    assert_eq!(c.top[0].basis[0].kind, BasisKind::Due);
    assert_eq!(c.top[1].item.as_deref(), Some("i-1a2b3c4d"));
    assert_eq!(c.proposals.len(), 3);
    assert!(matches!(&c.proposals[0], Proposal::Sort(s) if s.action == SortAction::Link));
    assert_eq!(c.notes.unwrap().base, base_of(b""));
    // A plan with nothing to rank is fine.
    assert!(check(&plan(vec![], vec![])).unwrap().top.is_empty());
}

#[test]
fn the_proposals_keep_their_kind_when_stored() {
    let c = check(&plan(vec![], vec![delegate("work")])).unwrap();
    let stored = serde_json::to_value(&c.proposals).unwrap();
    assert_eq!(stored[0]["kind"], "delegate");
    assert_eq!(stored[0]["scope"], "compare");
    let back: Vec<Proposal> = serde_json::from_value(stored).unwrap();
    assert_eq!(back, c.proposals);
}

#[test]
fn four_rows_are_refused() {
    let rows = (1..=4).map(|n| row(&format!("w-0000000{n}"))).collect();
    let reasons = refused(&plan(rows, vec![]));
    assert!(
        reasons.iter().any(|r| r.contains("top has more than 3")),
        "{reasons:?}"
    );
}

#[test]
fn bad_minutes_are_refused_by_row() {
    for minutes in [
        json!({"low": 50, "estimated": 40, "high": 60}),
        json!({"low": 10, "estimated": 70, "high": 60}),
        json!({"low": 10, "estimated": 20, "high": 1441}),
    ] {
        let mut r = row("w-0b1c2d3e");
        r["minutes"] = minutes.clone();
        let reasons = refused(&plan(vec![row("w-00000001"), r], vec![]));
        assert_eq!(reasons.len(), 1, "{minutes}: {reasons:?}");
        assert!(reasons[0].starts_with("top[1].minutes"), "{reasons:?}");
    }
    let mut r = row("w-0b1c2d3e");
    r["minutes"] = json!({"low": 1.5, "estimated": 2, "high": 3});
    assert!(refused(&plan(vec![r], vec![]))[0].contains("does not match the schema"));
}

#[test]
fn two_delegates_to_one_workspace_are_refused() {
    let reasons = refused(&plan(vec![], vec![delegate("work"), delegate("work")]));
    assert_eq!(reasons.len(), 1, "{reasons:?}");
    assert!(reasons[0].starts_with("proposals[1] asks the same workspace as proposals[0]"));
}

#[test]
fn field_counterexamples_name_the_field_and_never_the_value() {
    let mut both = row("w-0b1c2d3e");
    both["item"] = json!("i-1a2b3c4d");
    let mut bad_kind = row("w-0b1c2d3e");
    bad_kind["basis"] = json!([{"kind": MARK, "text": "x"}]);
    let mut long_purpose = delegate("work");
    long_purpose["purpose"] = json!(format!("{MARK}{}", "x".repeat(1024)));
    let cases: Vec<(Value, &str)> = vec![
        (plan(vec![both], vec![]), "top[0] must name exactly one"),
        (
            plan(vec![row(MARK)], vec![]),
            "top[0].work must be a Work item's id",
        ),
        (
            plan(vec![row("w-0b1c2d3e"), row("w-0b1c2d3e")], vec![]),
            "top[1] names the same item as top[0]",
        ),
        (plan(vec![bad_kind], vec![]), "top[0].basis[0].kind"),
        (plan(vec![], vec![long_purpose]), "proposals[0].purpose"),
        (
            plan(
                vec![],
                vec![json!({"kind": "sort", "capture": "c-4e5f6a7b", "action": "link"})],
            ),
            "proposals[0].work must be a Work item's id with action link",
        ),
        (
            plan(vec![], vec![json!({"kind": "pay", "to": MARK})]),
            "proposals[0] does not match the schema",
        ),
        (
            plan(
                vec![],
                vec![json!({"kind": "sort", "capture": "c-4e5f6a7b",
                                     "action": "note", "extra": MARK})],
            ),
            "proposals[0] does not match the schema",
        ),
    ];
    for (v, needle) in cases {
        let reasons = refused(&v);
        assert!(
            reasons.iter().any(|r| r.contains(needle)),
            "{needle}: {reasons:?}"
        );
    }
}

#[test]
fn the_hosts_fields_and_unknown_fields_are_refused() {
    for field in ["dueSoon", "unanswered", "inputsAt", "delivery", MARK] {
        let mut v = plan(vec![], vec![]);
        v[field] = json!([]);
        let reasons = refused(&v);
        assert!(
            reasons[0].contains("does not match the schema"),
            "{field}: {reasons:?}"
        );
    }
    let mut v = plan(vec![], vec![]);
    v["plan"] = json!("p-ffffffff");
    assert!(refused(&v)[0].contains("plan must be p-0a1b2c3d"));
}

#[test]
fn notes_over_16_kib_are_refused_and_the_copy_proposes_nothing() {
    let mut v = plan(vec![], vec![]);
    v["notes"] = json!("x".repeat(16 * 1024 + 1));
    assert!(refused(&v)[0].starts_with("notes must be at most 16 KiB"));
    v["notes"] = json!("");
    assert_eq!(check(&v).unwrap().notes, None, "the text the run read");
}

#[test]
fn a_symlinked_or_oversized_file_is_refused() {
    let dir = std::env::temp_dir().join(format!(
        "muxa-plan-file-{}-{:08x}",
        std::process::id(),
        getrandom::u32().unwrap()
    ));
    crate::assistant::record::prepare(&dir).unwrap();
    let missing = read(&dir, &run()).unwrap_err();
    assert!(missing[0].contains("does not exist. Write .muxa/assistant/plan.json"));
    let target = dir.join("elsewhere.json");
    std::fs::write(&target, plan(vec![], vec![]).to_string()).unwrap();
    std::os::unix::fs::symlink(&target, dir.join(PLAN)).unwrap();
    assert!(read(&dir, &run()).unwrap_err()[0].contains("symlink"));
    std::fs::remove_file(dir.join(PLAN)).unwrap();
    std::fs::write(dir.join(PLAN), vec![b' '; 64 * 1024 + 1]).unwrap();
    assert!(read(&dir, &run()).unwrap_err()[0].contains("larger than 64 KiB"));
    std::fs::write(dir.join(PLAN), b"{\"version\": 1\0}").unwrap();
    assert!(read(&dir, &run()).unwrap_err()[0].contains("NUL"));
    std::fs::write(dir.join(PLAN), plan(vec![], vec![]).to_string()).unwrap();
    assert!(read(&dir, &run()).is_ok());
    std::fs::remove_dir_all(dir).unwrap();
}
