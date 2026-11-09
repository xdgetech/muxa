use muxa_protocol::{AssistResult, AssistScope};
use serde_json::json;

use super::*;
use crate::assistant::record::RunFor;
use crate::records::{Loop, LoopSpec};

const RUN: &str = "t-00000a";

fn request(delivery: Delivery) -> AssistantRequest {
    AssistantRequest {
        id: "q-0000000a".into(),
        work: "w-0000000b".into(),
        version: 2,
        workspace: "w".into(),
        scope: AssistScope::Research,
        delivery,
        delivery_at: 5,
        created_at: 1,
        updated_at: 1,
        purpose: None,
        run: Some(RUN.into()),
        results: Vec::new(),
    }
}

/// A started run of version 2: its agent began at 100, its prompt was typed.
fn record() -> Record {
    let mut r: Record = serde_json::from_value(json!({
        "version": 1, "id": RUN, "rev": 3, "workspace": "w", "title": "t",
        "goal": "g", "status": "open", "createdAt": 90, "updatedAt": 100,
        "runs": [{"id": 1, "reason": "start", "startedAt": 100}],
    }))
    .unwrap();
    r.extra.extend(
        RunFor {
            request: "q-0000000a".into(),
            version: 2,
            work: "w-0000000b".into(),
        }
        .fields(),
    );
    r
}

fn facts(record: &Record, agent: Option<Agent>, now: u64) -> Facts<'_> {
    Facts {
        record: Some(record),
        agent,
        timeout: 1000,
        now,
    }
}

#[test]
fn the_record_and_the_adapter_decide_until_a_result() {
    let r = record();
    let q = request(Delivery::Pending);
    assert_eq!(derive(&q, &facts(&r, None, 200)), (Delivery::Received, 200));
    assert_eq!(
        derive(&q, &facts(&r, Some(Agent::Busy(150)), 200)),
        (Delivery::Processing, 150)
    );
    let busy = request(Delivery::Processing);
    // A list has no adapter: it keeps what the tick saw, with its time.
    assert_eq!(
        derive(&busy, &facts(&r, None, 300)),
        (Delivery::Processing, 5)
    );
    assert_eq!(
        derive(&busy, &facts(&r, Some(Agent::NotBusy), 300)),
        (Delivery::Received, 300)
    );
    let mut waiting = r.clone();
    waiting.extra.insert(
        "pendingPrompt".into(),
        json!({"kind": "start", "since": 100, "run": 1}),
    );
    assert_eq!(derive(&q, &facts(&waiting, None, 120)).0, Delivery::Pending);
    let mut launching = r.clone();
    launching.runs.clear();
    assert_eq!(
        derive(&q, &facts(&launching, None, 120)).0,
        Delivery::Pending
    );
}

#[test]
fn an_interrupted_send_is_unconfirmed_and_a_send_in_flight_is_not() {
    let mut r = record();
    r.extra.insert(
        crate::records::lifecycle::FIELD.into(),
        json!({"version": 1, "id": "delivery-1", "phase": "delivering", "sourceRun": 1,
               "reservedRun": 1, "pane": 3, "sourcePid": 9, "worker": null, "since": 100}),
    );
    let q = request(Delivery::Pending);
    assert_eq!(derive(&q, &facts(&r, None, 105)), (Delivery::Pending, 5));
    assert_eq!(
        derive(&q, &facts(&r, None, 200)),
        (Delivery::Unconfirmed, 100)
    );
    r.extra.insert(
        crate::records::lifecycle::FIELD.into(),
        json!({"version": 99}),
    );
    assert_eq!(derive(&q, &facts(&r, None, 200)).0, Delivery::Unconfirmed);
}

#[test]
fn no_result_after_the_timeout_is_timed_out_and_a_failed_check_is_failed() {
    let r = record();
    let q = request(Delivery::Processing);
    assert_eq!(
        derive(&q, &facts(&r, Some(Agent::Busy(150)), 1100)),
        (Delivery::TimedOut, 1100)
    );
    let mut stopped = r.clone();
    let spec = LoopSpec {
        check: "x".into(),
        max_attempts: Some(3),
        timeout: Some(60),
    };
    let lp = Loop::start(spec, None, false, 100).stopped(Reason::Limit, 400);
    stopped.set_loop(Some(&lp)).unwrap();
    assert_eq!(
        derive(&q, &facts(&stopped, None, 500)),
        (Delivery::Failed, 400)
    );
}

#[test]
fn a_result_cancel_or_another_version_is_never_derived_over() {
    let r = record();
    for d in [
        Delivery::Resulted,
        Delivery::Failed,
        Delivery::Cancelled,
        Delivery::CancelRequested,
    ] {
        assert_eq!(derive(&request(d), &facts(&r, None, 2000)), (d, 5));
    }
    let mut q = request(Delivery::TimedOut);
    q.results.push(AssistResult {
        version: 2,
        at: 1500,
        summary: "s".into(),
        stale: false,
        findings: Vec::new(),
        open_questions: Vec::new(),
        next: Vec::new(),
        files: Vec::new(),
        possible_duplicates: Vec::new(),
    });
    assert_eq!(
        derive(&q, &facts(&r, None, 2000)),
        (Delivery::Resulted, 1500)
    );
    let mut newer = request(Delivery::Pending);
    newer.version = 3;
    assert_eq!(
        derive(&newer, &facts(&r, None, 2000)),
        (Delivery::Pending, 5)
    );
}
