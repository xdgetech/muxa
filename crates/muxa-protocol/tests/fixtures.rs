//! Every fixture line decodes and re-encodes to exactly the same line. The
//! Swift and other clients test against the same files.

use muxa_protocol::{Message, Request};
use serde::Serialize;
use serde::de::DeserializeOwned;

fn round_trip<T: DeserializeOwned + Serialize>(file: &str) -> usize {
    let path = format!(
        "{}/../../protocol/fixtures/{file}",
        env!("CARGO_MANIFEST_DIR")
    );
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path}: {e}"));
    let mut n = 0;
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let value: T = serde_json::from_str(line).unwrap_or_else(|e| panic!("{file}: {e}\n{line}"));
        assert_eq!(
            serde_json::to_string(&value).unwrap(),
            line,
            "{file} does not round-trip"
        );
        n += 1;
    }
    n
}

#[test]
fn requests_round_trip() {
    assert_eq!(round_trip::<Request>("requests.jsonl"), 36);
}

#[test]
fn messages_round_trip() {
    assert_eq!(round_trip::<Message>("messages.jsonl"), 41);
}

#[test]
fn update_task_needs_a_status() {
    let old: Request = serde_json::from_str(
        r#"{"type":"update-task","id":1,"task":"t-000001","rev":2,"status":"done"}"#,
    )
    .unwrap();
    let Request::UpdateTask { status, .. } = old else {
        panic!()
    };
    assert_eq!(status, muxa_protocol::TaskStatus::Done);
    // ADR 0033 removed Move down and Later: an older app's queue request is refused.
    for line in [
        r#"{"type":"update-task","id":1,"task":"t-000001","rev":2,"inbox":{"down":true}}"#,
        r#"{"type":"update-task","id":1,"task":"t-000001","rev":2}"#,
    ] {
        assert!(serde_json::from_str::<Request>(line).is_err(), "{line}");
    }
}

#[test]
fn an_old_record_with_an_inbox_place_still_reads() {
    let line = r#"{"type":"task-saved","task":1,"record":{"id":"t-7f3a2c","rev":5,"workspace":"app","title":"Fix it","goal":"Fix it.","status":"open","createdAt":1767300000,"updatedAt":1767300400,"inbox":{"order":1767300400123,"since":1767300400}}}"#;
    let Message::TaskSaved { record, .. } = serde_json::from_str(line).unwrap() else {
        panic!()
    };
    assert!(
        serde_json::to_value(&record)
            .unwrap()
            .get("inbox")
            .is_none()
    );
}

#[test]
fn bytes_are_base64() {
    let line = r#"{"type":"bytes","id":7,"pane":"%12","data":"G1tBw6k="}"#;
    let Request::Bytes { data, .. } = serde_json::from_str(line).unwrap() else {
        panic!()
    };
    assert_eq!(data, b"\x1b[A\xc3\xa9");
}

#[test]
fn unknown_fields_are_ignored_and_unknown_types_rejected() {
    assert!(serde_json::from_str::<Request>(r#"{"type":"ping","id":1,"later":true}"#).is_ok());
    assert!(serde_json::from_str::<Request>(r#"{"type":"rm-rf","id":1}"#).is_err());
}

#[test]
fn workspace_checks_are_any_or_a_list() {
    use muxa_protocol::{WorkspaceChecks, WorkspaceInfo};
    let any: WorkspaceInfo = serde_json::from_str(r#"{"name":"w","checks":"any"}"#).unwrap();
    assert_eq!(any.checks, Some(WorkspaceChecks::Any));
    let none: WorkspaceInfo = serde_json::from_str(r#"{"name":"w"}"#).unwrap();
    assert_eq!(none.checks, None);
    assert!(serde_json::from_str::<WorkspaceInfo>(r#"{"name":"w","checks":"all"}"#).is_err());
}

#[test]
fn worker_contract_fixtures_round_trip() {
    use muxa_protocol::AgentRun;
    assert_eq!(round_trip::<Request>("workers/requests.jsonl"), 7);
    assert_eq!(round_trip::<Message>("workers/messages.jsonl"), 5);
    // A legacy client omits the worker; null is read the same way.
    for (field, expected) in [
        ("", None),
        (r#", "worker":null"#, None),
        (r#", "worker":"default""#, Some("default")),
        (r#", "worker":"Reviewer""#, Some("Reviewer")),
    ] {
        let request: Request = serde_json::from_str(&format!(
            r#"{{"type":"restart-agent","id":1,"task":"t-000001"{field}}}"#
        ))
        .unwrap();
        let Request::RestartAgent { worker, .. } = request else {
            panic!()
        };
        assert_eq!(worker.as_deref(), expected);
    }
    for bad in ["true", "17", "{}", "[]"] {
        assert!(
            serde_json::from_str::<Request>(&format!(
                r#"{{"type":"restart-agent","id":1,"task":"t-000001","worker":{bad}}}"#
            ))
            .is_err()
        );
    }
    // A legacy run has no worker: unrecorded, not the workspace default.
    let legacy: AgentRun =
        serde_json::from_str(r#"{"id":1,"startedAt":1,"reason":"start"}"#).unwrap();
    assert_eq!(legacy.worker, None);
}

#[test]
fn restart_recovery_fixtures_preserve_public_identity_and_unknown_values() {
    use muxa_protocol::{RestartOperation, RestartRecoveryAction};
    assert_eq!(round_trip::<Request>("restart-recovery/requests.jsonl"), 3);
    assert_eq!(round_trip::<Message>("restart-recovery/messages.jsonl"), 10);
    for (phase, advertised, permitted) in [
        ("prepared", "cancel-prepared", true),
        ("dispatching", "leave-unconfirmed", true),
        ("replaced", "finish-replacement", true),
        ("delivering", "leave-unconfirmed", true),
        ("prepared", "finish-replacement", false),
        ("resolved", "leave-unconfirmed", false),
        ("future-phase", "cancel-prepared", false),
    ] {
        let operation = RestartOperation {
            id: "fixture".into(),
            phase: phase.into(),
            worker: None,
            actions: vec![advertised.into()],
        };
        let action = RestartRecoveryAction::from_wire(advertised).unwrap();
        assert_eq!(operation.allows(action), permitted);
        assert!(
            !RestartOperation {
                actions: Vec::new(),
                ..operation
            }
            .allows(action)
        );
    }
    assert!(RestartRecoveryAction::from_wire("future-action").is_none());
    for action in ["future-action", "restart-agent", "", "null"] {
        let value = serde_json::json!({"type":"recover-restart","id":1,"task":"t-000001","rev":7,"operation":"fixture","action":action});
        assert!(serde_json::from_value::<Request>(value).is_err());
    }
    let operation: RestartOperation =
        serde_json::from_str(r#"{"id":"future","phase":"unknown","actions":["future-action"]}"#)
            .unwrap();
    assert_eq!(operation.actions, ["future-action"]);
}

#[test]
fn workspace_fixtures_round_trip() {
    use muxa_protocol::{WORK_ASK, work_level};
    assert_eq!(round_trip::<Request>("workspaces/requests.jsonl"), 2);
    assert_eq!(round_trip::<Message>("workspaces/messages.jsonl"), 3);
    let path = format!(
        "{}/../../protocol/fixtures/workspaces/messages.jsonl",
        env!("CARGO_MANIFEST_DIR")
    );
    let text = std::fs::read_to_string(path).unwrap();
    let lines: Vec<Message> = text
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    let Message::Hello { capabilities, .. } = &lines[0] else {
        panic!()
    };
    assert_eq!(work_level(capabilities), WORK_ASK);
    let Message::TaskList { workspaces, .. } = &lines[1] else {
        panic!()
    };
    assert_eq!(workspaces[0].root.as_deref(), Some("/work/lisk"));
    assert_eq!(workspaces[1].repos, Vec::<String>::new());
    // A bridge before ADR 0031 sends no root.
    let old: muxa_protocol::WorkspaceInfo = serde_json::from_str(r#"{"name":"w"}"#).unwrap();
    assert_eq!(old.root, None);
}

#[test]
fn ask_close_fixtures_round_trip() {
    use muxa_protocol::{WORK_CLOSE_ASK, work_level};
    assert_eq!(round_trip::<Request>("ask-close/requests.jsonl"), 2);
    assert_eq!(round_trip::<Message>("ask-close/messages.jsonl"), 4);
    let path = format!(
        "{}/../../protocol/fixtures/ask-close/messages.jsonl",
        env!("CARGO_MANIFEST_DIR")
    );
    let text = std::fs::read_to_string(path).unwrap();
    let lines: Vec<Message> = text
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    let Message::Hello { capabilities, .. } = &lines[0] else {
        panic!()
    };
    assert_eq!(work_level(capabilities), WORK_CLOSE_ASK);
    let Message::Result {
        sessions: Some(sessions),
        ..
    } = &lines[1]
    else {
        panic!()
    };
    assert_eq!(sessions[0].ask, None);
    assert_eq!(sessions[1].ask.as_deref(), Some("lisk"));
    assert_eq!(sessions[1].worktree, None);
}

#[test]
fn worktree_end_fixtures_round_trip() {
    use muxa_protocol::{ErrorCode, WORK_END_REMOVES, WorktreeEnd, work_level};
    assert_eq!(round_trip::<Request>("worktree-end/requests.jsonl"), 3);
    assert_eq!(round_trip::<Message>("worktree-end/messages.jsonl"), 4);
    let read = |name: &str| {
        std::fs::read_to_string(format!(
            "{}/../../protocol/fixtures/worktree-end/{name}",
            env!("CARGO_MANIFEST_DIR")
        ))
        .unwrap()
    };
    let requests: Vec<Request> = read("requests.jsonl")
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    assert!(matches!(
        requests[1],
        Request::UpdateTask {
            worktree: Some(WorktreeEnd::Force),
            ..
        }
    ));
    let messages: Vec<Message> = read("messages.jsonl")
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    let Message::Hello { capabilities, .. } = &messages[0] else {
        panic!()
    };
    assert_eq!(work_level(capabilities), WORK_END_REMOVES);
    let Message::Result {
        error: Some(error), ..
    } = &messages[1]
    else {
        panic!()
    };
    assert_eq!(error.code, ErrorCode::WouldLoseWork);
    // A client before level 7 sends no `worktree`; the worktree stays.
    let old: Request = serde_json::from_str(
        r#"{"type":"update-task","id":1,"task":"t-7f3a2c","rev":3,"status":"done"}"#,
    )
    .unwrap();
    assert!(matches!(old, Request::UpdateTask { worktree: None, .. }));
}

#[test]
fn routine_contract_fixtures_round_trip() {
    use muxa_protocol::{Disposition, InboxAction, work_level};
    assert_eq!(round_trip::<Request>("routines/requests.jsonl"), 6);
    assert_eq!(round_trip::<Message>("routines/messages.jsonl"), 6);
    let path = format!(
        "{}/../../protocol/fixtures/routines/messages.jsonl",
        env!("CARGO_MANIFEST_DIR")
    );
    let text = std::fs::read_to_string(path).unwrap();
    let mut lines = text.lines();
    let Ok(Message::Hello { capabilities, .. }) = serde_json::from_str(lines.next().unwrap())
    else {
        panic!()
    };
    assert_eq!(work_level(&capabilities), 1);
    let later: Request = serde_json::from_str(
        r#"{"type":"update-inbox","id":1,"entry":"e-00000000","action":"later","until":5}"#,
    )
    .unwrap();
    assert!(matches!(
        later,
        Request::UpdateInbox {
            action: InboxAction::Later,
            until: Some(5),
            ..
        }
    ));
    let Ok(Message::InboxList { entries, .. }) = serde_json::from_str(lines.nth(2).unwrap()) else {
        panic!()
    };
    assert_eq!(
        entries.iter().map(|e| e.disposition).collect::<Vec<_>>(),
        [Disposition::Open, Disposition::Later]
    );
    assert!(entries[0].proposal.is_some() && entries[1].proposal.is_none());
}

#[test]
fn item_fixtures_round_trip() {
    use muxa_protocol::{InboxAction, ItemKind, ItemState, WORK_ITEMS, work_level};
    assert_eq!(round_trip::<Request>("items/requests.jsonl"), 17);
    assert_eq!(round_trip::<Message>("items/messages.jsonl"), 5);
    let path = format!(
        "{}/../../protocol/fixtures/items/messages.jsonl",
        env!("CARGO_MANIFEST_DIR")
    );
    let text = std::fs::read_to_string(path).unwrap();
    let mut lines = text.lines();
    let Ok(Message::Hello { capabilities, .. }) = serde_json::from_str(lines.next().unwrap())
    else {
        panic!()
    };
    assert_eq!(work_level(&capabilities), WORK_ITEMS);
    let Ok(Message::InboxList { items, .. }) = serde_json::from_str(lines.next().unwrap()) else {
        panic!()
    };
    let kinds: Vec<ItemKind> = items.iter().map(|i| i.kind).collect();
    use ItemKind::*;
    assert_eq!(kinds, [Ask, Review, Failure, Proposal, Review, Ask]);
    assert_eq!(items[1].answers[2], InboxAction::KeepOpen);
    assert_eq!(
        (items[2].state, items[2].later_at),
        (ItemState::Open, Some(1_762_160_500))
    );
    // An old client's update-inbox has no text or option.
    let old: Request = serde_json::from_str(
        r#"{"type":"update-inbox","id":1,"entry":"e-00000000","action":"dismiss"}"#,
    )
    .unwrap();
    assert!(matches!(
        old,
        Request::UpdateInbox {
            text: None,
            option: None,
            ..
        }
    ));
}

/// ADR 0033 §4a: the `reply` answer, in the item fixtures.
#[test]
fn reply_fixtures_round_trip() {
    use muxa_protocol::{InboxAction, ItemKind};
    let path = format!(
        "{}/../../protocol/fixtures/items/requests.jsonl",
        env!("CARGO_MANIFEST_DIR")
    );
    let replies = std::fs::read_to_string(path)
        .unwrap()
        .lines()
        .filter_map(|l| serde_json::from_str::<Request>(l).ok())
        .filter(|r| {
            matches!(
                r,
                Request::UpdateInbox { action: InboxAction::Reply, text: Some(t), .. } if !t.is_empty()
            )
        })
        .count();
    assert_eq!(replies, 3);
    let path = format!(
        "{}/../../protocol/fixtures/items/messages.jsonl",
        env!("CARGO_MANIFEST_DIR")
    );
    let text = std::fs::read_to_string(path).unwrap();
    let Some(Ok(Message::InboxList { items, .. })) = text.lines().nth(1).map(serde_json::from_str)
    else {
        panic!()
    };
    use InboxAction::*;
    let answers = |id: &str| {
        items
            .iter()
            .find(|i| i.id == id)
            .map(|i| i.answers.clone())
            .unwrap()
    };
    assert_eq!(answers("i-0a1b2c3d"), [Approve, Reject, Reply, Later]);
    assert_eq!(answers("i-2c3d4e5f"), [Accept, Reply, KeepOpen, Later]);
    assert_eq!(answers("i-3d4e5f6a"), [Retry, Reply, Drop, Dismiss, Later]);
    let proposal = items.iter().find(|i| i.kind == ItemKind::Proposal).unwrap();
    assert!(!proposal.answers.contains(&Reply));
}

#[test]
fn reading_fixtures_round_trip() {
    use muxa_protocol::{InboxAction, ItemKind, WORK_READ, work_level};
    assert_eq!(round_trip::<Request>("reading/requests.jsonl"), 6);
    assert_eq!(round_trip::<Message>("reading/messages.jsonl"), 5);
    let path = format!(
        "{}/../../protocol/fixtures/reading/messages.jsonl",
        env!("CARGO_MANIFEST_DIR")
    );
    let text = std::fs::read_to_string(path).unwrap();
    let lines: Vec<Message> = text
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    let Message::Hello { capabilities, .. } = &lines[0] else {
        panic!()
    };
    assert_eq!(work_level(capabilities), WORK_READ);
    let Message::TaskSaved { record, .. } = &lines[1] else {
        panic!()
    };
    assert!(record.read && record.branch.is_none() && record.repo.is_some());
    let Message::InboxList { items, .. } = &lines[2] else {
        panic!()
    };
    assert_eq!(items[0].kind, ItemKind::Proposal);
    assert_eq!(
        items[0].answers,
        [InboxAction::Branch, InboxAction::Keep, InboxAction::Later]
    );
    let Message::TaskSaved { record, .. } = &lines[3] else {
        panic!()
    };
    assert!(!record.read && record.branch.is_some());
    // An older record has no `read`.
    let old: muxa_protocol::TaskRecord = serde_json::from_str(
        r#"{"id":"t-1","rev":1,"workspace":"w","title":"t","goal":"","status":"open","createdAt":0,"updatedAt":0}"#,
    )
    .unwrap();
    assert!(!old.read);
}

#[test]
fn assistant_fixtures_round_trip() {
    use muxa_protocol::{
        CaptureState, Delivery, SortAction, WORK_ASSISTANT, WorkAction, WorkState, work_level,
    };
    assert_eq!(round_trip::<Request>("assistant/requests.jsonl"), 44);
    assert_eq!(round_trip::<Message>("assistant/messages.jsonl"), 49);
    let read = |name: &str| {
        std::fs::read_to_string(format!(
            "{}/../../protocol/fixtures/assistant/{name}",
            env!("CARGO_MANIFEST_DIR")
        ))
        .unwrap()
    };
    let requests: Vec<Request> = read("requests.jsonl")
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    // A resent capture carries the same clientId; the host keeps one.
    let ids: Vec<&str> = requests[..2]
        .iter()
        .map(|r| match r {
            Request::Capture { client_id, .. } => client_id.as_str(),
            _ => panic!(),
        })
        .collect();
    assert_eq!(ids[0], ids[1]);
    assert!(matches!(
        requests[4],
        Request::SortCapture {
            action: SortAction::Link,
            work: Some(_),
            ..
        }
    ));
    assert!(matches!(
        requests[11],
        Request::UpdateWork {
            action: WorkAction::Merge,
            into: Some(_),
            ..
        }
    ));
    assert!(matches!(
        &requests[15],
        Request::CreateTask { work: Some(w), work_rev: Some(3), .. } if w == "w-6a7b8c9d"
    ));
    assert!(matches!(&requests[16], Request::GetWork { work, .. } if work == "w-6a7b8c9d"));
    let messages: Vec<Message> = read("messages.jsonl")
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    let Message::Hello { capabilities, .. } = &messages[0] else {
        panic!()
    };
    assert_eq!(work_level(capabilities), WORK_ASSISTANT);
    let Message::InboxList {
        work,
        captures,
        requests,
        ..
    } = &messages[6]
    else {
        panic!()
    };
    use WorkState::*;
    assert_eq!(
        work.iter().map(|w| w.state).collect::<Vec<_>>(),
        [WaitingDecision, InProgress, Open, Open, Done, Withdrawn]
    );
    assert!(work[3].later_at.is_some() && work[5].merged_into.is_some());
    assert_eq!(captures.len(), 1);
    assert_eq!(
        requests.iter().map(|r| r.delivery).collect::<Vec<_>>(),
        [
            Delivery::Resulted,
            Delivery::Processing,
            Delivery::Unconfirmed,
            Delivery::TimedOut
        ]
    );
    let results = &requests[0].results;
    assert!(!results[0].stale && results[1].stale);
    assert!(results[0].findings.iter().all(|f| !f.sources.is_empty()));
    // A client before level 9 reads an inbox-list without the new arrays.
    let old: Message =
        serde_json::from_str(r#"{"type":"inbox-list","inbox":1,"entries":[]}"#).unwrap();
    assert!(matches!(old, Message::InboxList { work, .. } if work.is_empty()));
    // get-work: every linked Capture, sorted ones included, in the item's order.
    let Message::WorkDetail {
        entry,
        captures,
        requests,
        ..
    } = &messages[9]
    else {
        panic!()
    };
    assert_eq!(
        captures.iter().map(|c| &c.id).collect::<Vec<_>>(),
        entry.captures.iter().collect::<Vec<_>>()
    );
    assert!(captures.iter().all(|c| c.state == CaptureState::Sorted));
    assert_eq!(requests[0].id, entry.requests[0]);
    let Message::Result { error: Some(e), .. } = &messages[11] else {
        panic!()
    };
    assert_eq!(e.code, muxa_protocol::ErrorCode::BadRequest);
}

#[test]
fn plan_fixtures_round_trip() {
    use muxa_protocol::{
        BasisKind, ChangeKind, Delivery, ErrorCode, ItemKind, OverallRequests, Share, SortAction,
        WORK_PLANS, WorkAction, work_level,
    };
    let read = |name: &str| -> Vec<String> {
        std::fs::read_to_string(format!(
            "{}/../../protocol/fixtures/assistant/{name}",
            env!("CARGO_MANIFEST_DIR")
        ))
        .unwrap()
        .lines()
        .map(str::to_owned)
        .collect()
    };
    let requests: Vec<Request> = read("requests.jsonl")
        .iter()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    assert!(matches!(
        &requests[18],
        Request::ApplyNotes { source, version: 1, .. } if source.starts_with("q-")
    ));
    assert!(matches!(
        &requests[19],
        Request::ApplyNotes { source, version: 2, .. } if source.starts_with("p-")
    ));
    assert!(matches!(
        requests[20],
        Request::PlanNow {
            minutes: Some(45),
            ..
        }
    ));
    assert!(matches!(
        requests[21],
        Request::PlanNow { minutes: None, .. }
    ));
    assert!(matches!(
        &requests[22],
        Request::UpdateWork { action: WorkAction::Edit, due: Some(d), .. } if d.timezone == "Asia/Tokyo"
    ));
    assert!(matches!(
        requests[23],
        Request::UpdateWork {
            action: WorkAction::Pin,
            ..
        }
    ));
    assert!(matches!(
        requests[24],
        Request::UpdateWork {
            action: WorkAction::Unpin,
            ..
        }
    ));

    let messages: Vec<Message> = read("messages.jsonl")
        .iter()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    let Message::Hello { capabilities, .. } = &messages[12] else {
        panic!()
    };
    assert_eq!(work_level(capabilities), WORK_PLANS);
    // apply-notes on a request answers with the request whose result proposed them.
    let Message::WorkSaved {
        request: Some(q), ..
    } = &messages[13]
    else {
        panic!()
    };
    let notes = q.results[0].notes.as_ref().unwrap();
    assert_eq!(notes.base.len(), 64);
    let Message::Result { error: Some(e), .. } = &messages[14] else {
        panic!()
    };
    assert_eq!(e.code, ErrorCode::Conflict);
    let Message::PlanSaved { plan: 420, entry } = &messages[15] else {
        panic!()
    };
    assert_eq!(
        (entry.delivery, entry.minutes),
        (Delivery::Pending, Some(45))
    );
    assert!(entry.top.is_empty());
    let Message::Result { error: Some(e), .. } = &messages[16] else {
        panic!()
    };
    assert_eq!(e.code, ErrorCode::Unsupported);
    let Message::WorkSaved { entry, .. } = &messages[17] else {
        panic!()
    };
    assert!(entry.pinned_at.is_some() && entry.due.is_some());

    let Message::InboxList {
        items,
        work,
        plans,
        assistant: Some(assistant),
        ..
    } = &messages[18]
    else {
        panic!()
    };
    assert_eq!(
        items.iter().map(|i| i.kind).collect::<Vec<_>>(),
        [ItemKind::Sort, ItemKind::Delegate]
    );
    assert_eq!(items[0].sort.as_ref().unwrap().action, SortAction::Work);
    assert!(items[1].delegate.is_some() && items[0].delegate.is_none());
    // Pinned first; a Later item is never in a plan's top, only in dueSoon.
    assert!(work[0].pinned_at.is_some());
    let plan = &plans[0];
    assert!(plans[0].at > plans[1].at);
    assert!(plan.top.len() <= 3);
    assert_eq!(plan.top[0].work.as_deref(), Some(work[0].id.as_str()));
    let later: Vec<&str> = work
        .iter()
        .filter(|w| w.later_at.is_some())
        .map(|w| w.id.as_str())
        .collect();
    assert!(
        plan.top
            .iter()
            .all(|r| r.work.as_deref().is_none_or(|w| !later.contains(&w)))
    );
    assert_eq!(plan.due_soon, later);
    assert!(
        plan.top
            .iter()
            .all(|r| r.work.is_some() != r.item.is_some())
    );
    assert!(
        plan.top
            .iter()
            .all(|r| r.minutes.low <= r.minutes.estimated && r.minutes.estimated <= r.minutes.high)
    );
    assert_eq!(plan.top[0].basis[0].kind, BasisKind::Manual);
    assert_eq!(
        plan.changed.iter().map(|c| c.change).collect::<Vec<_>>(),
        [
            ChangeKind::Moved,
            ChangeKind::Entered,
            ChangeKind::Entered,
            ChangeKind::Left
        ]
    );
    assert!(!plan.unanswered.is_empty() && !plan.stale.is_empty());
    assert_eq!(assistant.overall.as_deref(), Some("secretary"));
    assert_eq!(
        assistant
            .delegates
            .iter()
            .map(|d| (d.share, d.overall_requests))
            .collect::<Vec<_>>(),
        [
            (Share::Summaries, OverallRequests::Review),
            (Share::Outline, OverallRequests::Auto)
        ]
    );
    // Approving a delegate item: the request it made, the answered item, then the result.
    assert!(matches!(
        &messages[19],
        Message::WorkSaved {
            request: Some(_),
            ..
        }
    ));
    let Message::InboxSaved {
        item: Some(item), ..
    } = &messages[20]
    else {
        panic!()
    };
    assert!(item.answers.is_empty() && item.delegate.is_some());
    // A plan whose run did not start says why, in host text.
    let Message::PlanSaved { entry, .. } = &messages[22] else {
        panic!()
    };
    assert_eq!(entry.delivery, Delivery::Failed);
    assert!(entry.reason.as_deref().is_some_and(|r| r.len() <= 500));
    assert!(plans.iter().all(|p| p.reason.is_none()));
    // A client before level 10 reads an inbox-list without plans.
    let old: Message =
        serde_json::from_str(r#"{"type":"inbox-list","inbox":1,"entries":[]}"#).unwrap();
    assert!(
        matches!(old, Message::InboxList { plans, assistant, .. } if plans.is_empty() && assistant.is_none())
    );
}

#[test]
fn policy_fixtures_round_trip() {
    use muxa_protocol::{
        BasisKind, Confidence, CorrectionKind, ErrorCode, HypothesisState, InboxAction, ItemKind,
        ItemState, PlanAction, PolicyAction, WORK_POLICIES, work_level,
    };
    let read = |name: &str| -> Vec<String> {
        std::fs::read_to_string(format!(
            "{}/../../protocol/fixtures/assistant/{name}",
            env!("CARGO_MANIFEST_DIR")
        ))
        .unwrap()
        .lines()
        .map(str::to_owned)
        .collect()
    };
    let requests: Vec<Request> = read("requests.jsonl")
        .iter()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    assert!(matches!(
        &requests[27],
        Request::AddPolicy { scope, until: Some(_), .. } if scope == "all"
    ));
    assert!(matches!(
        &requests[29],
        Request::UpdatePolicy {
            action: PolicyAction::Edit,
            text: Some(_),
            ..
        }
    ));
    assert!(matches!(
        &requests[30],
        Request::UpdatePolicy {
            action: PolicyAction::Delete,
            text: None,
            ..
        }
    ));
    assert!(matches!(
        &requests[31],
        Request::UpdatePlan { action: PlanAction::Reorder, order, row: None, .. } if order.len() == 3
    ));
    assert!(matches!(
        &requests[32],
        Request::UpdatePlan {
            action: PlanAction::Dismiss,
            row: Some(_),
            text: Some(_),
            ..
        }
    ));
    assert!(matches!(
        &requests[34],
        Request::UpdateInbox {
            action: InboxAction::Reject,
            text: Some(_),
            ..
        }
    ));
    assert!(matches!(
        requests[35],
        Request::UpdateInbox {
            action: InboxAction::Confirm,
            ..
        }
    ));
    let forgotten: Vec<&str> = requests[36..39]
        .iter()
        .map(|r| match r {
            Request::Forget { record, .. } => record.as_str(),
            _ => panic!(),
        })
        .collect();
    assert_eq!(forgotten, ["x-0c1d2e3f", "h-2b3c4d5e", "h-3c4d5e6f"]);

    let messages: Vec<Message> = read("messages.jsonl")
        .iter()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    let Message::Hello { capabilities, .. } = &messages[23] else {
        panic!()
    };
    assert_eq!(work_level(capabilities), WORK_POLICIES);
    let Message::PolicySaved { policy: 430, entry } = &messages[24] else {
        panic!()
    };
    assert!(entry.id.starts_with("y-") && entry.source == "user" && entry.text.len() <= 500);
    let Message::Result { error: Some(e), .. } = &messages[26] else {
        panic!()
    };
    assert_eq!(e.code, ErrorCode::Conflict);
    let Message::PolicySaved { entry, .. } = &messages[27] else {
        panic!()
    };
    assert_eq!(entry.rev, 2);
    // A delete answers with the result only.
    assert!(matches!(
        messages[28],
        Message::Result {
            id: Some(433),
            error: None,
            ..
        }
    ));
    let Message::PlanSaved { entry: plan, .. } = &messages[29] else {
        panic!()
    };
    assert_eq!(plan.user_order.len(), plan.top.len());
    assert_eq!(plan.dismissed, ["i-1a2b3c4d"]);
    let kinds: Vec<BasisKind> = plan
        .top
        .iter()
        .flat_map(|r| r.basis.iter().map(|b| b.kind))
        .collect();
    for kind in [
        BasisKind::Policy,
        BasisKind::Hypothesis,
        BasisKind::Correction,
    ] {
        assert!(kinds.contains(&kind));
    }
    let Message::Result { error: Some(e), .. } = &messages[30] else {
        panic!()
    };
    assert_eq!(e.code, ErrorCode::Conflict);
    let Message::InboxSaved {
        item: Some(rejected),
        ..
    } = &messages[31]
    else {
        panic!()
    };
    assert_eq!(rejected.answer, Some(InboxAction::Reject));
    // Confirm: the policy it made, then the answered item.
    let Message::PolicySaved { entry: made, .. } = &messages[32] else {
        panic!()
    };
    let Message::InboxSaved {
        item: Some(confirmed),
        ..
    } = &messages[33]
    else {
        panic!()
    };
    assert_eq!(
        Some(made.source.as_str()),
        confirmed.hypothesis.as_ref().map(|h| h.hypothesis.as_str())
    );
    let Message::Result { error: Some(e), .. } = &messages[35] else {
        panic!()
    };
    assert_eq!(e.code, ErrorCode::BadRequest);

    let Message::InboxList {
        items,
        policies,
        hypotheses,
        corrections,
        ..
    } = &messages[36]
    else {
        panic!()
    };
    assert_eq!(
        (items[0].kind, items[0].state),
        (ItemKind::Hypothesis, ItemState::Open)
    );
    assert_eq!(
        items[0].answers,
        [
            InboxAction::Confirm,
            InboxAction::Reject,
            InboxAction::Later
        ]
    );
    assert_eq!(
        items[0].hypothesis.as_ref().map(|h| h.confidence),
        Some(Confidence::Low)
    );
    assert!(policies.iter().all(|p| p.id.starts_with("y-")));
    assert_eq!(
        hypotheses.iter().map(|h| h.state).collect::<Vec<_>>(),
        [HypothesisState::Proposed, HypothesisState::Rejected]
    );
    assert!(
        hypotheses
            .iter()
            .all(|h| (1..=5).contains(&h.evidence.len()))
    );
    assert_eq!(
        corrections.iter().map(|c| c.kind).collect::<Vec<_>>(),
        [
            CorrectionKind::Dismissed,
            CorrectionKind::Reordered,
            CorrectionKind::Rejected,
            CorrectionKind::Deferred,
            CorrectionKind::Pinned
        ]
    );
    assert!(corrections.windows(2).all(|w| w[0].at >= w[1].at));
    let reordered = &corrections[1];
    assert!(reordered.row.is_none() && reordered.before.len() == reordered.after.len());
    assert!(corrections[3].reason.is_none());
    // A client before level 11 reads an inbox-list without the new arrays.
    let old: Message =
        serde_json::from_str(r#"{"type":"inbox-list","inbox":1,"entries":[]}"#).unwrap();
    assert!(matches!(old, Message::InboxList { policies, .. } if policies.is_empty()));
}

#[test]
fn staff_fixtures_round_trip() {
    use muxa_protocol::{
        Delivery, ErrorCode, InboxAction, ItemKind, StepBy, WORK_STAFF, work_level,
    };
    let read = |name: &str| -> Vec<String> {
        std::fs::read_to_string(format!(
            "{}/../../protocol/fixtures/assistant/{name}",
            env!("CARGO_MANIFEST_DIR")
        ))
        .unwrap()
        .lines()
        .map(str::to_owned)
        .collect()
    };
    let requests: Vec<Request> = read("requests.jsonl")
        .iter()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    assert!(matches!(
        &requests[39],
        Request::RequestAssistant { worker: Some(w), .. } if w == "lisk-strategist"
    ));
    assert!(matches!(
        &requests[40],
        Request::Capture { assist: Some(a), .. } if a.worker.as_deref() == Some("lisk-architect")
    ));
    assert!(matches!(
        requests[42],
        Request::ApplyNotes { step: Some(2), .. }
    ));
    // Older lines carry no worker and no step.
    assert!(matches!(
        requests[18],
        Request::ApplyNotes { step: None, .. }
    ));

    let messages: Vec<Message> = read("messages.jsonl")
        .iter()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    let Message::Hello { capabilities, .. } = &messages[37] else {
        panic!()
    };
    assert_eq!(work_level(capabilities), WORK_STAFF);
    // A named worker is step 1, picked by the user.
    let Message::WorkSaved {
        request: Some(q), ..
    } = &messages[38]
    else {
        panic!()
    };
    assert_eq!(
        (q.steps.len(), q.steps[0].by, q.steps[0].run.as_deref()),
        (1, StepBy::User, q.run.as_deref())
    );
    // Approving a hand-off: the request with its new step, then the answered item.
    let Message::WorkSaved {
        request: Some(chain),
        ..
    } = &messages[42]
    else {
        panic!()
    };
    let newest = chain.steps.last().unwrap();
    assert_eq!(
        (newest.step, newest.by, newest.delivery),
        (2, StepBy::Handoff, chain.delivery)
    );
    assert_eq!(newest.run, chain.run);
    assert_eq!(
        chain.steps[0].handoff.as_ref().map(|h| h.worker.as_str()),
        Some(newest.worker.as_str())
    );
    let Message::InboxSaved {
        item: Some(item), ..
    } = &messages[43]
    else {
        panic!()
    };
    let handoff = item.handoff.as_ref().unwrap();
    assert_eq!(
        (item.kind, item.answer, handoff.step),
        (ItemKind::Handoff, Some(InboxAction::Approve), 1)
    );
    assert!(item.answers.is_empty());
    // Each result names its worker and step; inside a version nothing is stale.
    let Message::WorkSaved {
        request: Some(done),
        ..
    } = &messages[45]
    else {
        panic!()
    };
    assert_eq!(done.delivery, Delivery::Resulted);
    let current: Vec<(Option<u32>, bool)> = done
        .results
        .iter()
        .filter(|r| r.version == done.version)
        .map(|r| (r.step, r.stale))
        .collect();
    assert_eq!(current, [(Some(2), false), (Some(1), false)]);
    assert!(done.results[0].notes.is_some() && done.results[0].worker.is_some());
    let Message::Result { error: Some(e), .. } = &messages[47] else {
        panic!()
    };
    assert_eq!(e.code, ErrorCode::BadRequest);

    let Message::InboxList {
        items,
        assistant: Some(assistant),
        ..
    } = &messages[48]
    else {
        panic!()
    };
    assert_eq!(
        items.iter().map(|i| i.kind).collect::<Vec<_>>(),
        [ItemKind::Handoff, ItemKind::Delegate]
    );
    assert_eq!(
        items[0].answers,
        [
            InboxAction::Approve,
            InboxAction::Reject,
            InboxAction::Later
        ]
    );
    assert_eq!(
        items[1].delegate.as_ref().and_then(|d| d.worker.as_deref()),
        Some("lisk-security")
    );
    let staff = &assistant.delegates[0].staff;
    assert_eq!(staff.iter().filter(|m| m.lead).count(), 1);
    assert!(staff.iter().all(|m| m.description.len() <= 1024));
    // A client before level 12 reads an inbox-list without staff.
    let old: Message =
        serde_json::from_str(r#"{"type":"inbox-list","inbox":1,"entries":[],"assistant":{"delegates":[{"workspace":"w","share":"outline","overallRequests":"review"}]}}"#).unwrap();
    assert!(
        matches!(old, Message::InboxList { assistant: Some(a), .. } if a.delegates[0].staff.is_empty())
    );
}

/// A level 11 client knows no steps, staff or result worker: it reads the
/// newest step as the request and ignores the rest.
#[test]
fn a_level_11_client_reads_a_chain_as_one_request() {
    #[derive(serde::Deserialize)]
    struct OldResult {
        version: u64,
        summary: String,
    }
    #[derive(serde::Deserialize)]
    struct OldRequest {
        delivery: muxa_protocol::Delivery,
        run: Option<String>,
        results: Vec<OldResult>,
    }
    let line = std::fs::read_to_string(format!(
        "{}/../../protocol/fixtures/assistant/messages.jsonl",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap()
    .lines()
    .nth(45)
    .unwrap()
    .to_owned();
    let value: serde_json::Value = serde_json::from_str(&line).unwrap();
    let old: OldRequest = serde_json::from_value(value["request"].clone()).unwrap();
    assert_eq!(old.delivery, muxa_protocol::Delivery::Resulted);
    assert_eq!(old.run.as_deref(), Some("t-00e004"));
    assert!(
        old.results
            .iter()
            .all(|r| r.version >= 1 && !r.summary.is_empty())
    );
    // A newer host's `by` is unknown, not a failed list.
    let mut list: serde_json::Value = serde_json::from_str(
        std::fs::read_to_string(format!(
            "{}/../../protocol/fixtures/assistant/messages.jsonl",
            env!("CARGO_MANIFEST_DIR")
        ))
        .unwrap()
        .lines()
        .nth(48)
        .unwrap(),
    )
    .unwrap();
    list["requests"][0]["steps"][0]["by"] = serde_json::json!("routine");
    let Message::InboxList { requests, .. } = serde_json::from_value(list).unwrap() else {
        panic!()
    };
    assert_eq!(requests[0].steps[0].by, muxa_protocol::StepBy::Unknown);
}

/// The fixtures' `inbox-list` lines with the values a newer host may add: an
/// older client still reads the whole list, and sees each new value as unknown.
#[test]
fn a_newer_hosts_values_decode_as_unknown() {
    use muxa_protocol::{
        AssistScope, BasisKind, ChangeKind, Confidence, CorrectionKind, Delivery, HypothesisState,
        InboxAction, ItemKind, ItemState, OverallRequests, Share, WorkState,
    };
    use serde_json::{Value, json};
    let lists: Vec<Value> = std::fs::read_to_string(format!(
        "{}/../../protocol/fixtures/assistant/messages.jsonl",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap()
    .lines()
    .map(|l| serde_json::from_str::<Value>(l).unwrap())
    .filter(|v| v["type"] == "inbox-list")
    .collect();
    let mut plans = lists[1].clone();
    plans["items"][0]["kind"] = json!("reminder");
    plans["items"][0]["state"] = json!("snoozed");
    plans["items"][0]["answers"] = json!(["approve", "snooze", "later"]);
    plans["work"][0]["state"] = json!("blocked");
    plans["captures"][0]["source"] = json!("mail");
    plans["requests"][0]["delivery"] = json!("queued");
    plans["requests"][0]["scope"] = json!("summarize");
    plans["plans"][0]["delivery"] = json!("queued");
    plans["plans"][0]["top"][0]["basis"][0]["kind"] = json!("calendar");
    plans["plans"][0]["changed"][0]["change"] = json!("split");
    plans["assistant"]["delegates"][0]["share"] = json!("everything");
    plans["assistant"]["delegates"][0]["overallRequests"] = json!("ask-first");
    let Message::InboxList {
        items,
        work,
        captures,
        requests,
        plans,
        assistant: Some(assistant),
        ..
    } = serde_json::from_value(plans).unwrap()
    else {
        panic!()
    };
    assert_eq!(
        (items[0].kind, items[0].state),
        (ItemKind::Unknown, ItemState::Unknown)
    );
    assert_eq!(
        items[0].answers,
        [
            InboxAction::Approve,
            InboxAction::Unknown,
            InboxAction::Later
        ]
    );
    assert_eq!(work[0].state, WorkState::Unknown);
    assert_eq!(captures[0].source, muxa_protocol::CaptureSource::Unknown);
    assert_eq!(
        (requests[0].delivery, requests[0].scope),
        (Delivery::Unknown, AssistScope::Unknown)
    );
    assert_eq!(plans[0].delivery, Delivery::Unknown);
    assert_eq!(plans[0].top[0].basis[0].kind, BasisKind::Unknown);
    assert_eq!(plans[0].changed[0].change, ChangeKind::Unknown);
    assert_eq!(
        (
            assistant.delegates[0].share,
            assistant.delegates[0].overall_requests
        ),
        (Share::Unknown, OverallRequests::Unknown)
    );

    let mut policies = lists[2].clone();
    policies["hypotheses"][0]["state"] = json!("superseded");
    policies["hypotheses"][0]["confidence"] = json!("high");
    policies["corrections"][0]["kind"] = json!("snoozed");
    let Message::InboxList {
        hypotheses,
        corrections,
        ..
    } = serde_json::from_value(policies).unwrap()
    else {
        panic!()
    };
    assert_eq!(
        (hypotheses[0].state, hypotheses[0].confidence),
        (HypothesisState::Unknown, Confidence::Unknown)
    );
    assert_eq!(corrections[0].kind, CorrectionKind::Unknown);

    // Tasks, errors and panes too.
    let task = r#"{"type":"task-saved","task":1,"record":{"id":"t-000001","rev":1,"workspace":"w","title":"T","goal":"G","status":"paused","createdAt":1,"updatedAt":1,"runs":[{"id":1,"startedAt":1,"reason":"resume"}]}}"#;
    let Message::TaskSaved { record, .. } = serde_json::from_str(task).unwrap() else {
        panic!()
    };
    assert_eq!(record.status, muxa_protocol::TaskStatus::Unknown);
    assert_eq!(record.runs[0].reason, muxa_protocol::RunReason::Unknown);
    let error = r#"{"type":"result","id":1,"error":{"code":"rate-limited","message":"m"}}"#;
    let Message::Result { error: Some(e), .. } = serde_json::from_str(error).unwrap() else {
        panic!()
    };
    assert_eq!(e.code, muxa_protocol::ErrorCode::Unknown);
}

/// A request names only values the host knows: a newer client's value, or
/// `unknown` itself, is still a decode error, so the bridge refuses it.
#[test]
fn a_request_with_an_unknown_value_is_refused() {
    for line in [
        r#"{"type":"update-inbox","id":1,"entry":"i-1a2b3c4d","action":"snooze"}"#,
        r#"{"type":"update-inbox","id":1,"entry":"i-1a2b3c4d","action":"unknown"}"#,
        r#"{"type":"update-task","id":1,"task":"t-000001","rev":2,"status":"unknown"}"#,
        r#"{"type":"capture","id":1,"clientId":"x","text":"t","source":"mail"}"#,
        r#"{"type":"capture","id":1,"clientId":"x","text":"t","workspace":"w","assist":{"scope":"summarize"}}"#,
        r#"{"type":"sort-capture","id":1,"capture":"c-0a1b2c3d","rev":1,"action":"unknown"}"#,
        r#"{"type":"request-assistant","id":1,"work":"w-0a1b2c3d","rev":1,"scope":"summarize"}"#,
    ] {
        assert!(serde_json::from_str::<Request>(line).is_err(), "{line}");
    }
    let known = r#"{"type":"capture","id":1,"clientId":"x","text":"t","source":"tmux","assist":{"scope":"draft"}}"#;
    assert!(serde_json::from_str::<Request>(known).is_ok());
}

#[test]
fn inbox_delete_fixtures_round_trip() {
    use muxa_protocol::{InboxAction, WORK_DELETE_INBOX, work_level};
    assert_eq!(round_trip::<Request>("inbox-delete/requests.jsonl"), 5);
    assert_eq!(round_trip::<Message>("inbox-delete/messages.jsonl"), 6);
    let path = format!(
        "{}/../../protocol/fixtures/inbox-delete/messages.jsonl",
        env!("CARGO_MANIFEST_DIR")
    );
    let text = std::fs::read_to_string(path).unwrap();
    let lines: Vec<Message> = text
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    let Message::Hello { capabilities, .. } = &lines[0] else {
        panic!()
    };
    assert_eq!(work_level(capabilities), WORK_DELETE_INBOX);
    let Message::InboxSaved {
        item: Some(item), ..
    } = &lines[1]
    else {
        panic!()
    };
    // Protect is accepted on any item and never listed.
    assert!(item.protected_at.is_some());
    assert!(!item.answers.contains(&InboxAction::Protect));
    assert!(matches!(
        lines[3],
        Message::InboxDeleted {
            inbox: 403,
            deleted: 12
        }
    ));
}
