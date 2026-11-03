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
    use muxa_protocol::{WorkerAssignment, WorkerSelection};
    assert_eq!(round_trip::<Request>("workers/requests.jsonl"), 6);
    assert_eq!(round_trip::<Message>("workers/messages.jsonl"), 5);
    for (field, expected) in [
        ("", WorkerSelection::Keep),
        (r#", "worker":null"#, WorkerSelection::WorkspaceDefault),
        (
            r#", "worker":"Reviewer""#,
            WorkerSelection::Named("Reviewer".into()),
        ),
    ] {
        let request: Request = serde_json::from_str(&format!(
            r#"{{"type":"restart-agent","id":1,"task":"t-000001"{field}}}"#
        ))
        .unwrap();
        let Request::RestartAgent { worker, .. } = request else {
            panic!()
        };
        assert_eq!(worker, expected);
    }
    for bad in ["true", "17", "{}", "[]"] {
        assert!(
            serde_json::from_str::<Request>(&format!(
                r#"{{"type":"restart-agent","id":1,"task":"t-000001","worker":{bad}}}"#
            ))
            .is_err()
        );
    }
    let unknown: WorkerAssignment =
        serde_json::from_str(r#"{"kind":"future-assignment","name":"Scheduler"}"#).unwrap();
    assert!(!unknown.is_known());
    let create: Request = serde_json::from_str(
        r#"{"type":"create-task","id":1,"workspace":"w","title":"t","goal":"g","worker":null}"#,
    )
    .unwrap();
    assert!(matches!(create, Request::CreateTask { worker: None, .. }));
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
            assignment: None,
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
