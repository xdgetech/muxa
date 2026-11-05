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
    assert_eq!(round_trip::<Request>("requests.jsonl"), 34);
}

#[test]
fn messages_round_trip() {
    assert_eq!(round_trip::<Message>("messages.jsonl"), 39);
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
    assert_eq!(round_trip::<Request>("items/requests.jsonl"), 16);
    assert_eq!(round_trip::<Message>("items/messages.jsonl"), 4);
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
    assert_eq!(items[1].answers[3], InboxAction::ChangeCourse);
    assert_eq!((items[2].state, items[2].later_at), (ItemState::Open, Some(1_762_160_500)));
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
    assert_eq!(
        answers("i-2c3d4e5f"),
        [Accept, Reply, Redo, ChangeCourse, KeepOpen, Later]
    );
    assert_eq!(
        answers("i-3d4e5f6a"),
        [Retry, Reply, ChangeCourse, Drop, Dismiss, Later]
    );
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
