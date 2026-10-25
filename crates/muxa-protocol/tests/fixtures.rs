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
    assert_eq!(round_trip::<Request>("requests.jsonl"), 15);
}

#[test]
fn messages_round_trip() {
    assert_eq!(round_trip::<Message>("messages.jsonl"), 20);
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
