use super::*;
use crate::assistant::record::{RequestRun, RunFor};
use crate::records::Status;

fn record(id: &str, run: RequestRun, goal: &str) -> Record {
    let mut r: Record = serde_json::from_value(serde_json::json!({
        "version": 1, "id": id, "rev": 1, "workspace": "w", "title": "t",
        "goal": goal, "status": "open", "createdAt": 1, "updatedAt": 1,
    }))
    .unwrap();
    r.extra.extend(RunFor::Request(run).fields());
    assert_eq!(r.status, Status::Open);
    r
}

fn run(version: u64, step: Option<u32>) -> RequestRun {
    RequestRun {
        request: "q-0000000a".into(),
        version,
        work: "w-0000000b".into(),
        notes_base: None,
        step,
        member: None,
        shared_base: None,
    }
}

#[test]
fn the_first_step_keeps_the_marker_of_a_request_without_staff() {
    assert_eq!(marker("q-0000000a", 2, 1), "muxa-request: q-0000000a v2");
    assert_eq!(marker("q-0000000a", 2, 3), "muxa-request: q-0000000a v2 s3");
}

#[test]
fn a_run_is_found_again_only_for_its_own_step() {
    let records = [
        record(
            "t-00000a",
            run(1, None),
            "goal\nmuxa-request: q-0000000a v1",
        ),
        record(
            "t-00000b",
            run(1, Some(2)),
            "goal\nmuxa-request: q-0000000a v1 s2",
        ),
        // A goal that names step 3 without being made for it claims nothing.
        record(
            "t-00000c",
            run(1, Some(2)),
            "goal\nmuxa-request: q-0000000a v1 s3",
        ),
    ];
    let found = |step| existing(&records, "q-0000000a", 1, step).map(|r| r.id.as_str());
    assert_eq!(found(1), Some("t-00000a"));
    assert_eq!(found(2), Some("t-00000b"));
    assert_eq!(found(3), None);
    assert_eq!(existing(&records, "q-0000000a", 2, 1), None);
}

#[test]
fn a_claim_holds_one_step_and_an_old_claim_is_step_one() {
    let mut extra = Extra::new();
    extra.insert(CREATING_FIELD.into(), claim(2, 3, 50));
    assert_eq!(creating(&extra, 2, 3), Some(50));
    assert_eq!(creating(&extra, 2, 2), None);
    assert_eq!(creating(&extra, 1, 3), None);
    extra.insert(CREATING_FIELD.into(), serde_json::json!([2, 40]));
    assert_eq!(creating(&extra, 2, 1), Some(40));
    assert_eq!(creating(&extra, 2, 2), None);
}

#[test]
fn the_newest_step_carries_the_requests_delivery_and_run() {
    let mut q: AssistantRequest = serde_json::from_value(serde_json::json!({
        "id": "q-0000000a", "work": "w-0000000b", "version": 1, "workspace": "w",
        "scope": "research", "delivery": "processing", "deliveryAt": 9,
        "createdAt": 1, "updatedAt": 1, "run": "t-00000b",
    }))
    .unwrap();
    assert_eq!(current(&q), 1);
    sync(&mut q);
    assert!(q.steps.is_empty(), "a request without staff keeps no steps");
    q.steps = first(Some("lisk-strategist"), StepBy::User, 5);
    q.steps.push(fresh(2, "lisk-architect", StepBy::Handoff, 6));
    sync(&mut q);
    assert_eq!(current(&q), 2);
    let last = q.steps.last().unwrap();
    assert_eq!(
        (last.delivery, last.delivery_at, last.run.as_deref()),
        (Delivery::Processing, 9, Some("t-00000b"))
    );
    assert_eq!(q.steps[0].delivery, Delivery::Pending);
    assert!(first(None, StepBy::Lead, 5).is_empty());
}
