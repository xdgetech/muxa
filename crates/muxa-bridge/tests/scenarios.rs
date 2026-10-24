//! Replays recorded bridge streams (protocol/fixtures/scenarios) into a
//! terminal emulator. No tmux needed. Other clients replay the same files.

mod common;

use common::Term;
use muxa_protocol::Message;

#[test]
fn recorded_streams_rebuild_the_expected_screen() {
    let dir = format!(
        "{}/../../protocol/fixtures/scenarios",
        env!("CARGO_MANIFEST_DIR")
    );
    let mut files: Vec<_> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("{dir}: {e}"))
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "jsonl"))
        .collect();
    files.sort();
    assert!(!files.is_empty(), "no scenarios in {dir}");

    for path in files {
        let text = std::fs::read_to_string(&path).unwrap();
        let mut lines = text.lines();
        let header: serde_json::Value =
            serde_json::from_str(lines.next().expect("header")).unwrap();
        let rows: Vec<String> = serde_json::from_value(header["expectRows"].clone()).unwrap();
        let cursor: (u16, u16) = serde_json::from_value(header["expectCursor"].clone()).unwrap();

        let mut term = Term::new();
        let mut watch = None;
        for line in lines {
            let msg: Message = serde_json::from_str(line).unwrap();
            if let Message::Screen { watch: w, .. } = &msg {
                watch = Some(*w);
            }
            term.apply(&msg, watch.expect("a scenario starts with a screen"));
        }
        assert_eq!(term.rows(), rows, "{}", path.display());
        assert_eq!(term.cursor(), cursor, "{}", path.display());
    }
}
