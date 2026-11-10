use serde_json::{Value, json};

use super::super::handoff::Chain;
use super::super::record::RequestRun;
use super::super::result::check as check_result;
use super::*;

/// Agent text that must never be echoed into a reason.
const SENTINEL: &str = "SENTINEL-5c1e";

fn run() -> RequestRun {
    RequestRun {
        request: "q-0000000a".into(),
        version: 2,
        work: "w-0000000b".into(),
        notes_base: None,
        step: None,
        member: None,
        shared_base: None,
    }
}

fn result(writes: Value) -> Value {
    json!({"version": 1, "request": "q-0000000a", "requestVersion": 2,
           "summary": "The note is ready.", "writes": writes})
}

fn write(action: &str, target: &str, content: &str) -> Value {
    json!({"action": action, "target": target, "content": content})
}

fn checked(writes: Value, takes: bool) -> Result<Vec<ResultWrite>, Vec<String>> {
    let chain = Chain {
        writes: takes,
        ..Chain::default()
    };
    let bytes = serde_json::to_vec(&result(writes)).unwrap();
    check_result(&bytes, &run(), &chain).map(|c| c.result.writes)
}

#[test]
fn writes_pass_with_the_routine_proposal_hash() {
    let content = "Release 2.4 is out.\n\n\t- Faster sync\n";
    let mut first = write("Post to #release", "slack:#release", content);
    first["reason"] = json!("The user asked for it.");
    let found = checked(
        json!([
            first,
            write("Comment", "https://linear.app/x/ENG-12", "Ships in 2.4.")
        ]),
        true,
    )
    .unwrap();
    assert_eq!(found.len(), 2);
    assert_eq!(
        found[0].hash,
        proposal_hash("Post to #release", "slack:#release", content)
    );
    assert_eq!(found[0].content, content);
    assert_eq!(found[0].reason.as_deref(), Some("The user asked for it."));
    assert!(found.iter().all(|w| w.item.is_none() && w.task.is_none()));
    // The limits themselves pass: 3 writes, 200 characters, 500 bytes, 8 KiB.
    let edge = write(
        &"é".repeat(MAX_ACTION),
        &"t".repeat(MAX_TARGET),
        &"c".repeat(MAX_CONTENT),
    );
    assert_eq!(
        checked(json!([edge.clone(), edge.clone(), edge]), true)
            .unwrap()
            .len(),
        3
    );
    // No writes: nothing to take, in any workspace.
    assert!(checked(json!([]), false).unwrap().is_empty());
}

#[test]
fn counterexamples_are_refused_without_quoting_the_agent() {
    let ok = || write("Post", "slack:#release", "Text");
    let mut long_reason = ok();
    long_reason["reason"] = json!(format!("{SENTINEL}{}", "r".repeat(MAX_LINE)));
    let mut extra = ok();
    extra["cc"] = json!(SENTINEL);
    let cases = [
        (
            json!([ok(), ok(), ok(), ok()]),
            "writes has more than 3 entries",
        ),
        (
            json!([write("Post", &format!("slack:#a\n{SENTINEL}"), "Text")]),
            "writes[0].target must be 1 to 500 bytes on one line",
        ),
        (
            json!([
                ok(),
                write(
                    "Post",
                    &format!("{SENTINEL}{}", "t".repeat(MAX_TARGET)),
                    "Text"
                )
            ]),
            "writes[1].target must be 1 to 500 bytes",
        ),
        (
            json!([write(&format!("Post\n{SENTINEL}"), "slack:#a", "Text")]),
            "writes[0].action must be 1 to 200 characters on one line",
        ),
        (
            json!([write(
                "Post",
                "slack:#a",
                &format!("{SENTINEL}{}", "c".repeat(MAX_CONTENT))
            )]),
            "writes[0].content must be 1 to 8192 bytes",
        ),
        (
            json!([write("Post", "slack:#a", &format!("{SENTINEL}\u{7}"))]),
            "writes[0].content must be 1 to 8192 bytes",
        ),
        (
            json!([write("Post", "slack:#a", "")]),
            "writes[0].content must be 1 to 8192 bytes",
        ),
        (
            json!([long_reason]),
            "writes[0].reason must be 1 to 500 bytes",
        ),
        (json!([extra]), "does not match the schema"),
    ];
    for (writes, needle) in cases {
        let reasons = checked(writes.clone(), true).unwrap_err();
        let all = reasons.join("\n");
        assert!(all.contains(needle), "{writes}: {all}");
        assert!(!all.contains(SENTINEL), "{all}");
    }
}

#[test]
fn a_workspace_without_a_writer_refuses_writes_and_says_where_they_go() {
    let reasons = checked(json!([write("Post", SENTINEL, SENTINEL)]), false).unwrap_err();
    let all = reasons.join("\n");
    assert!(all.contains("takes no write proposals"), "{all}");
    assert!(all.contains("in next instead"), "{all}");
    assert!(!all.contains(SENTINEL), "{all}");
    // A request without staff or configuration has no writer either.
    let bytes = serde_json::to_vec(&result(json!([write("Post", "t", "c")]))).unwrap();
    assert!(check_result(&bytes, &run(), &Chain::default()).is_err());
}
