use std::path::PathBuf;

use muxa_protocol::AssistScope;
use serde_json::{Value, json};

use super::*;

/// Agent text that must never be echoed into a reason.
const SENTINEL: &str = "SENTINEL-7f3a";

fn run() -> RunFor {
    RunFor {
        request: "q-0000000a".into(),
        version: 2,
        work: "w-0000000b".into(),
        notes_base: None,
    }
}

fn good() -> Value {
    json!({
        "version": 1,
        "request": "q-0000000a",
        "requestVersion": 2,
        "summary": "Two options fit.\nThe first is cheaper.",
        "findings": [{
            "text": "Plan A costs less.",
            "sources": [
                {"url": "https://example.com/a", "title": "Pricing", "readAt": 1_700_000_000u64},
                {"path": "/notes/a.md"}
            ]
        }],
        "openQuestions": ["Which region?"],
        "next": ["Ask for a quote."],
        "files": ["/root/assistant/x-t-000001/draft.md"],
        "possibleDuplicates": [{"work": "w-0000000c", "reason": "Same vendor."}]
    })
}

fn check(v: &Value) -> Result<AssistResult, Vec<String>> {
    validate(&serde_json::to_vec(v).unwrap(), &run())
}

#[test]
fn a_good_result_passes_with_the_run_version() {
    let r = check(&good()).unwrap();
    assert_eq!((r.version, r.at, r.stale), (2, 0, false));
    assert_eq!(r.findings[0].sources.len(), 2);
    assert_eq!(r.possible_duplicates[0].work, "w-0000000c");
    let mut minimal = good();
    for field in [
        "findings",
        "openQuestions",
        "next",
        "files",
        "possibleDuplicates",
    ] {
        minimal.as_object_mut().unwrap().remove(field);
    }
    assert!(check(&minimal).is_ok());
}

/// Sets `pointer` in a good result to `value`.
fn with(pointer: &str, value: Value) -> Value {
    let mut v = good();
    *v.pointer_mut(pointer).unwrap() = value;
    v
}

#[test]
fn counterexamples_are_refused_by_field_without_their_value() {
    let long = |n: usize| Value::String(format!("{SENTINEL}{}", "x".repeat(n)));
    let many = |n: usize, item: Value| Value::Array(vec![item; n]);
    let finding = good()["findings"][0].clone();
    let cases = [
        (with("/version", json!(2)), "version must be 1"),
        (
            with("/request", json!(SENTINEL)),
            "request must be q-0000000a",
        ),
        (
            with("/requestVersion", json!(1)),
            "requestVersion must be 2",
        ),
        (with("/summary", json!("  ")), "summary"),
        (with("/summary", long(4096)), "summary"),
        (
            with("/summary", json!(format!("{SENTINEL}\u{1b}"))),
            "summary",
        ),
        (with("/findings", many(21, finding)), "findings has more"),
        (with("/findings/0/text", long(1000)), "findings[0].text"),
        (
            with("/findings/0/sources", json!([])),
            "findings[0].sources",
        ),
        (
            with("/findings/0/sources/1", json!({"title": SENTINEL})),
            "findings[0].sources[1] needs",
        ),
        (
            with(
                "/findings/0/sources/0/url",
                json!(format!("ftp://{SENTINEL}")),
            ),
            "sources[0].url",
        ),
        (
            with("/findings/0/sources/1/path", json!(SENTINEL)),
            "sources[1].path",
        ),
        (
            with(
                "/findings/0/sources/0/title",
                json!(format!("{SENTINEL}\n")),
            ),
            "sources[0].title",
        ),
        (
            with("/openQuestions", many(11, json!("q"))),
            "openQuestions has more",
        ),
        (with("/openQuestions/0", long(500)), "openQuestions[0]"),
        (with("/next/0", json!("")), "next[0]"),
        (with("/files", many(21, json!("/a"))), "files has more"),
        (with("/files/0", json!(SENTINEL)), "files[0]"),
        (
            with(
                "/possibleDuplicates",
                many(6, json!({"work": "w-0000000c", "reason": "r"})),
            ),
            "possibleDuplicates has more",
        ),
        (
            with("/possibleDuplicates/0/work", json!(SENTINEL)),
            "possibleDuplicates[0].work",
        ),
        (
            with("/possibleDuplicates/0/work", json!("w-0000000b")),
            "possibleDuplicates[0].work",
        ),
        (
            with("/possibleDuplicates/0/work", json!("w-0000000C")),
            "possibleDuplicates[0].work",
        ),
        (
            with("/possibleDuplicates/0/reason", long(500)),
            "possibleDuplicates[0].reason",
        ),
    ];
    for (value, needle) in cases {
        let reasons = check(&value).unwrap_err();
        let all = reasons.join("\n");
        assert!(all.contains(needle), "{needle}: {all}");
        assert!(!all.contains(SENTINEL), "{needle}: {all}");
    }
}

#[test]
fn bytes_that_are_not_a_result_are_refused_without_their_value() {
    let mut unknown = good();
    unknown[SENTINEL] = json!(1);
    let mut missing = good();
    missing.as_object_mut().unwrap().remove("request");
    let mut wrong_type = good();
    wrong_type["requestVersion"] = json!(SENTINEL);
    let nul = serde_json::to_vec(&with("/summary", json!(format!("a\u{0}{SENTINEL}")))).unwrap();
    let cases: [(Vec<u8>, &str); 7] = [
        (b"{\"version\": 1, \0}".to_vec(), "NUL byte"),
        (nul, "summary"),
        (vec![b'"', 0xff, b'"'], "not UTF-8"),
        (
            format!("{{\"summary\": \"{SENTINEL}").into_bytes(),
            "ends too early",
        ),
        (
            serde_json::to_vec(&unknown).unwrap(),
            "does not match the schema",
        ),
        (
            serde_json::to_vec(&missing).unwrap(),
            "does not match the schema",
        ),
        (
            serde_json::to_vec(&wrong_type).unwrap(),
            "does not match the schema",
        ),
    ];
    for (bytes, needle) in cases {
        let all = validate(&bytes, &run()).unwrap_err().join("\n");
        assert!(all.contains(needle), "{needle}: {all}");
        assert!(!all.contains(SENTINEL), "{needle}: {all}");
    }
}

struct Dir(PathBuf);

impl Drop for Dir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn run_dir() -> Dir {
    let dir = std::env::temp_dir().join(format!(
        "muxa-assistant-result-{}-{:08x}",
        std::process::id(),
        getrandom::u32().unwrap()
    ));
    crate::assistant::record::prepare(&dir).unwrap();
    Dir(dir)
}

#[test]
fn the_file_is_read_from_a_plain_folder_only() {
    let d = run_dir();
    let missing = read(&d.0, &run()).unwrap_err();
    assert!(missing[0].contains("does not exist"), "{missing:?}");
    let file = d.0.join(RESULT);
    std::fs::write(&file, serde_json::to_vec(&good()).unwrap()).unwrap();
    assert!(read(&d.0, &run()).is_ok());

    let elsewhere = d.0.join("elsewhere.json");
    std::fs::rename(&file, &elsewhere).unwrap();
    std::os::unix::fs::symlink(&elsewhere, &file).unwrap();
    let linked = read(&d.0, &run()).unwrap_err();
    assert!(linked[0].contains("symlink"), "{linked:?}");
    std::fs::remove_file(&file).unwrap();

    let real = d.0.join("real");
    std::fs::rename(d.0.join(STAGE_DIR), &real).unwrap();
    std::fs::rename(&elsewhere, real.join("result.json")).unwrap();
    std::os::unix::fs::symlink(&real, d.0.join(STAGE_DIR)).unwrap();
    let linked = read(&d.0, &run()).unwrap_err();
    assert!(linked[0].contains("symlink"), "{linked:?}");
    std::fs::remove_file(d.0.join(STAGE_DIR)).unwrap();

    crate::assistant::record::prepare(&d.0).unwrap();
    std::fs::write(&file, vec![b' '; (64 << 10) + 1]).unwrap();
    let big = read(&d.0, &run()).unwrap_err();
    assert!(big[0].contains("larger than 64 KiB"), "{big:?}");
}

fn request(version: u64, results: Vec<AssistResult>) -> AssistantRequest {
    AssistantRequest {
        id: "q-0000000a".into(),
        work: "w-0000000b".into(),
        version,
        workspace: "w".into(),
        scope: AssistScope::Research,
        delivery: Delivery::Processing,
        delivery_at: 1,
        created_at: 1,
        updated_at: 1,
        purpose: None,
        run: Some("t-00000a".into()),
        results,
    }
}

fn result(version: u64) -> AssistResult {
    let mut r = check(&good()).unwrap();
    r.version = version;
    r
}

#[test]
fn a_new_result_is_stored_first_and_older_ones_go_stale() {
    let mut q = request(2, vec![result(1)]);
    assert_eq!(store(&mut q, result(2), 90), Ok(Stored::New));
    assert_eq!(
        (q.delivery, q.delivery_at, q.updated_at),
        (Delivery::Resulted, 90, 90)
    );
    assert_eq!(
        (q.results[0].version, q.results[0].at, q.results[0].stale),
        (2, 90, false)
    );
    assert!(q.results[1].stale);
}

#[test]
fn the_same_version_twice_changes_nothing() {
    let mut q = request(2, Vec::new());
    store(&mut q, result(2), 90).unwrap();
    let before = q.clone();
    assert_eq!(store(&mut q, result(2), 95), Ok(Stored::Already));
    assert_eq!(q, before);
}

#[test]
fn a_run_of_an_older_version_or_a_cancelled_request_is_not_stored() {
    let mut q = request(3, Vec::new());
    let before = q.clone();
    assert!(matches!(
        store(&mut q, result(2), 90),
        Err(Refused::NotCurrent {
            current: 3,
            run: 2,
            ..
        })
    ));
    q.delivery = Delivery::CancelRequested;
    q.version = 2;
    assert!(matches!(
        store(&mut q, result(2), 90),
        Err(Refused::Cancelled(_))
    ));
    assert_eq!(q.results, before.results);
}

#[test]
fn at_most_ten_results_are_kept() {
    let mut q = request(12, (1..=11).rev().map(result).collect());
    store(&mut q, result(12), 90).unwrap();
    assert_eq!(q.results.len(), MAX_RESULTS);
    assert_eq!(q.results[0].version, 12);
    assert_eq!(q.results[9].version, 3);
}

#[test]
fn notes_are_kept_with_the_runs_base_and_checked_for_size_and_nul() {
    let read = crate::assistant::notes::base_of(b"# Old\n");
    let with_base = RunFor {
        notes_base: Some(read.clone()),
        ..run()
    };
    let mut v = good();
    v["notes"] = json!(format!("# New\n{SENTINEL}\n"));
    let bytes = serde_json::to_vec(&v).unwrap();
    let notes = validate(&bytes, &with_base).unwrap().notes.unwrap();
    assert_eq!((notes.base.as_str(), notes.text.len()), (read.as_str(), 20));

    // A run that read no notes may not propose any.
    let all = validate(&bytes, &run()).unwrap_err().join("\n");
    assert!(all.contains("notes"), "{all}");
    assert!(!all.contains(SENTINEL), "{all}");
    for text in [
        format!("{SENTINEL}\u{0}"),
        format!("{SENTINEL}{}", "x".repeat(16 << 10)),
    ] {
        v["notes"] = json!(text);
        let bytes = serde_json::to_vec(&v).unwrap();
        let all = validate(&bytes, &with_base).unwrap_err().join("\n");
        assert!(all.contains("notes must be at most 16 KiB"), "{all}");
        assert!(!all.contains(SENTINEL), "{all}");
    }
    // An empty text clears the notes; the copy's own text proposes nothing.
    v["notes"] = json!("");
    let cleared = validate(&serde_json::to_vec(&v).unwrap(), &with_base).unwrap();
    assert_eq!(cleared.notes.unwrap().text, "");
    v["notes"] = json!("# Old\n");
    let same = validate(&serde_json::to_vec(&v).unwrap(), &with_base).unwrap();
    assert_eq!(same.notes, None);
}
