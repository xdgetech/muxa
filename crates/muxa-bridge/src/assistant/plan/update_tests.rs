use muxa_protocol::{Correction, Delivery};

use super::*;
use crate::assistant::corrections::tests::{NOW, host};

fn change(plan: &Plan, action: PlanAction) -> Change {
    Change {
        plan: plan.id.clone(),
        rev: plan.rev,
        action,
        order: Vec::new(),
        row: None,
        text: None,
    }
}

fn reorder(plan: &Plan, order: &[&str]) -> Change {
    Change {
        order: order.iter().map(|s| (*s).to_owned()).collect(),
        ..change(plan, PlanAction::Reorder)
    }
}

fn dismiss(plan: &Plan, row: &str, text: Option<&str>) -> Change {
    Change {
        row: Some(row.into()),
        text: text.map(str::to_owned),
        ..change(plan, PlanAction::Dismiss)
    }
}

fn code(r: Result<Plan, ErrorBody>) -> ErrorCode {
    r.unwrap_err().code
}

#[test]
fn a_reorder_sets_the_users_order_and_records_before_and_after() {
    let h = host();
    let rows = ["w-0000000a", "i-0000000b", "w-0000000c"];
    let plan = h.plan(100, Delivery::Resulted, &rows);
    let order = ["i-0000000b", "w-0000000a", "w-0000000c"];
    let saved = update(&h.config, reorder(&plan, &order), NOW).unwrap();
    assert_eq!(saved.user_order, order);
    assert_eq!(saved.rev, plan.rev + 1);
    let top: Vec<_> = saved.top.iter().filter_map(corrections::row_id).collect();
    assert_eq!(top, rows, "top keeps the agent's order");
    // The same order again changes nothing.
    let again = update(&h.config, reorder(&saved, &order), NOW).unwrap();
    assert_eq!(again.rev, saved.rev);
    let got: Vec<Correction> = h.corrections();
    assert_eq!(got.len(), 1);
    let c = &got[0];
    assert_eq!(
        (c.kind, c.row.as_ref(), c.in_top),
        (CorrectionKind::Reordered, None, true)
    );
    assert_eq!(
        (&c.before, &c.after),
        (
            &rows.map(String::from).to_vec(),
            &order.map(String::from).to_vec()
        )
    );
    assert_eq!(c.plan, plan.id);
}

#[test]
fn an_order_names_every_row_once() {
    let h = host();
    let plan = h.plan(100, Delivery::Resulted, &["w-0000000a", "w-0000000b"]);
    for order in [
        &["w-0000000a"][..],
        &["w-0000000a", "w-0000000a"],
        &["w-0000000a", "w-0000000b", "w-0000000c"],
        &["w-0000000a", "w-0000000c"],
    ] {
        assert_eq!(
            code(update(&h.config, reorder(&plan, order), NOW)),
            ErrorCode::BadRequest
        );
    }
    assert!(h.corrections().is_empty());
}

#[test]
fn a_dismiss_records_its_row_and_reason_once() {
    let h = host();
    let w = h.work(Some("work"));
    let plan = h.plan(100, Delivery::Resulted, &[&w.id, "i-0000000b"]);
    let saved = update(
        &h.config,
        dismiss(&plan, &w.id, Some("Waiting for the vendor.")),
        NOW,
    )
    .unwrap();
    assert_eq!(saved.dismissed, std::slice::from_ref(&w.id));
    let again = update(&h.config, dismiss(&saved, &w.id, None), NOW).unwrap();
    assert_eq!(again.rev, saved.rev, "a dismissed row stays dismissed");
    let bare = update(&h.config, dismiss(&saved, "i-0000000b", None), NOW).unwrap();
    assert_eq!(bare.dismissed.len(), 2);
    let got = h.corrections();
    assert_eq!(got.len(), 2);
    let with_reason = got
        .iter()
        .find(|c| c.row.as_deref() == Some(w.id.as_str()))
        .unwrap();
    assert_eq!(
        (
            with_reason.kind,
            with_reason.in_top,
            with_reason.workspace.as_deref()
        ),
        (CorrectionKind::Dismissed, true, Some("work"))
    );
    assert_eq!(
        with_reason.reason.as_deref(),
        Some("Waiting for the vendor.")
    );
    assert!(got.iter().any(|c| c.reason.is_none()));
    let unlisted = dismiss(&bare, "w-0000ffff", None);
    assert_eq!(
        code(update(&h.config, unlisted, NOW)),
        ErrorCode::BadRequest
    );
}

#[test]
fn only_the_newest_resulted_plan_at_its_rev_changes() {
    let h = host();
    let rows = ["w-0000000a", "w-0000000b"];
    let older = h.plan(100, Delivery::Resulted, &rows);
    let current = h.plan(200, Delivery::Resulted, &rows);
    // A plan still being made does not replace Today.
    h.plan(300, Delivery::Processing, &[]);
    let order = ["w-0000000b", "w-0000000a"];
    assert_eq!(
        code(update(&h.config, reorder(&older, &order), NOW)),
        ErrorCode::Conflict
    );
    let stale = Change {
        rev: current.rev - 1,
        ..reorder(&current, &order)
    };
    assert_eq!(code(update(&h.config, stale, NOW)), ErrorCode::Conflict);
    assert!(update(&h.config, reorder(&current, &order), NOW).is_ok());
    let failed = h.plan(400, Delivery::Failed, &[]);
    assert_eq!(
        code(update(&h.config, dismiss(&failed, "w-0000000a", None), NOW)),
        ErrorCode::Conflict
    );
    assert_eq!(h.corrections().len(), 1);
}

#[test]
fn each_action_takes_its_own_fields() {
    let h = host();
    let plan = h.plan(100, Delivery::Resulted, &["w-0000000a"]);
    let with_row = Change {
        row: Some("w-0000000a".into()),
        ..reorder(&plan, &["w-0000000a"])
    };
    let no_order = change(&plan, PlanAction::Reorder);
    let no_row = change(&plan, PlanAction::Dismiss);
    let with_order = Change {
        order: vec!["w-0000000a".into()],
        ..dismiss(&plan, "w-0000000a", None)
    };
    let long = "x".repeat(corrections::MAX_REASON + 1);
    let oversize = dismiss(&plan, "w-0000000a", Some(&long));
    let bad_id = Change {
        plan: "p-XYZ".into(),
        ..reorder(&plan, &["w-0000000a"])
    };
    let unknown = Change {
        plan: "p-0000ffff".into(),
        ..reorder(&plan, &["w-0000000a"])
    };
    for c in [
        with_row, no_order, no_row, with_order, oversize, bad_id, unknown,
    ] {
        assert_eq!(code(update(&h.config, c, NOW)), ErrorCode::BadRequest);
    }
    assert!(h.corrections().is_empty());
}
