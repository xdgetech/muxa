use muxa_protocol::{Disposition, RoutineRun};
use serde_json::json;

use super::*;

const SECRET: &str = "source-secret-4d2a";

fn stage(items: serde_json::Value) -> Vec<u8> {
    serde_json::to_vec(&json!({"version": 1, "items": items, "state": {"cursor": 7}})).unwrap()
}

fn briefing(key: &str) -> serde_json::Value {
    json!({"kind": "briefing", "key": key, "title": "Release thread",
           "body": format!("Moved.\n{SECRET}"),
           "links": [{"url": "https://example.com/t/1", "title": "Thread"}]})
}

fn proposal(key: &str) -> serde_json::Value {
    json!({"kind": "proposal", "key": key, "title": "Close ENG-12", "body": "Stale.",
           "action": "Close the issue", "target": "linear:ENG-12", "content": "Closing."})
}

fn state_with_run(run: &str) -> RoutineState {
    let mut s: RoutineState = serde_json::from_value(json!({
        "version": 1, "workspace": "w", "name": "Brief"
    }))
    .unwrap();
    s.running = Some(RoutineRun {
        occurrence: "2026-11-03T09:00".into(),
        started_at: 1,
        task: Some(run.into()),
    });
    s
}

fn ids() -> impl FnMut() -> Option<String> {
    let mut n = 0;
    move || {
        n += 1;
        Some(format!("e-{n:08x}"))
    }
}

#[test]
fn a_good_stage_validates_with_its_proposal_hash() {
    let out = validate(&stage(json!([briefing("a"), proposal("b")])), 10).unwrap();
    assert_eq!(out.items.len(), 2);
    assert_eq!(out.state, Some(json!({"cursor": 7})));
    let p = out.items[1].proposal.as_ref().unwrap();
    assert_eq!(
        p.hash,
        proposal_hash("Close the issue", "linear:ENG-12", "Closing.")
    );
    assert!(p.hash.starts_with("sha256:") && p.hash.len() == 7 + 64);
    assert_ne!(
        p.hash,
        proposal_hash("Close the issue", "linear:ENG-1", "2Closing.")
    );
    let none = validate(br#"{"version":1}"#, 10).unwrap();
    assert!(none.items.is_empty() && none.state.is_none());
}

#[test]
fn every_rule_is_checked_and_no_reason_quotes_the_source() {
    let long = "x".repeat(5000);
    let cases = [
        json!([{"kind": "note", "key": "k", "title": "t"}]),
        json!([{"kind": "briefing", "key": "", "title": "t"}]),
        json!([{"kind": "briefing", "key": format!("a\n{SECRET}"), "title": "t"}]),
        json!([{"kind": "briefing", "key": "k", "title": format!("two\nlines {SECRET}")}]),
        json!([{"kind": "briefing", "key": "k", "title": "t", "body": long}]),
        json!([{"kind": "briefing", "key": "k", "title": "t", "body": "bell\u{7}"}]),
        json!([{"kind": "briefing", "key": "k", "title": "t",
                "links": [{"url": format!("javascript:{SECRET}")}]}]),
        json!([{"kind": "briefing", "key": "k", "title": "t",
                "links": [{"url": "https://exa mple.com"}]}]),
        json!([{"kind": "briefing", "key": "k", "title": "t",
                "links": (0..11).map(|_| json!({"url": "https://e.com"})).collect::<Vec<_>>()}]),
        json!([{"kind": "briefing", "key": "k", "title": "t", "action": "a"}]),
        json!([{"kind": "proposal", "key": "k", "title": "t", "action": "a"}]),
        json!([{"kind": "proposal", "key": "k", "title": "t", "action": "a", "target": "t",
                "content": ""}]),
        json!([briefing("same"), proposal("same")]),
    ];
    for items in cases {
        let reasons = validate(&stage(items.clone()), 10).unwrap_err();
        assert!(!reasons.is_empty(), "{items}");
        for r in &reasons {
            assert!(!r.contains(SECRET), "{r}");
        }
    }
    let over = validate(&stage(json!([briefing("a"), briefing("b")])), 1).unwrap_err();
    assert_eq!(over.len(), 1);
    assert!(validate(br#"{"version":2}"#, 10).is_err());
    assert!(validate(br#"{"version":1,"extra":true}"#, 10).is_err());
    let big = json!({"version": 1, "state": "s".repeat(17 * 1024)});
    assert!(validate(&serde_json::to_vec(&big).unwrap(), 10).is_err());
}

#[test]
fn publish_stores_new_keys_commits_state_and_closes_the_run() {
    let mut s = state_with_run("t-000001");
    let out = validate(&stage(json!([briefing("a"), proposal("b")])), 10).unwrap();
    let n = publish(
        &mut s,
        "t-000001",
        "2026-11-03T09:00",
        out.clone(),
        50,
        ids(),
    )
    .unwrap();
    assert_eq!(n, 2);
    assert_eq!(s.running, None);
    assert_eq!(s.state, Some(json!({"cursor": 7})));
    assert_eq!(
        s.last_success
            .as_ref()
            .map(|x| (x.task.as_str(), x.published)),
        Some(("t-000001", 2))
    );
    assert!(
        s.entries
            .iter()
            .all(|e| e.entry.disposition == Disposition::Open)
    );
    // The same run again: nothing new, no error.
    let again = publish(
        &mut s,
        "t-000001",
        "2026-11-03T09:00",
        out.clone(),
        60,
        ids(),
    )
    .unwrap();
    assert_eq!((again, s.entries.len()), (0, 2));
    // The next run repeats key "a": only "c" is new, and its state stays when absent.
    s.running = Some(RoutineRun {
        occurrence: "2026-11-04T09:00".into(),
        started_at: 2,
        task: Some("t-000002".into()),
    });
    let next = Output {
        items: validate(&stage(json!([briefing("a"), briefing("c")])), 10)
            .unwrap()
            .items,
        state: None,
    };
    let mut fresh = ids();
    for _ in 0..2 {
        fresh();
    }
    assert_eq!(
        publish(&mut s, "t-000002", "2026-11-04T09:00", next, 70, fresh).unwrap(),
        1
    );
    assert_eq!(s.entries.len(), 3);
    assert_eq!(s.state, Some(json!({"cursor": 7})));
}

#[test]
fn deleted_entries_are_not_published_again_and_protected_ones_stay() {
    let mut s = state_with_run("t-000001");
    let out = validate(
        &stage(json!([briefing("a"), briefing("b"), proposal("c")])),
        10,
    )
    .unwrap();
    publish(&mut s, "t-000001", "2026-11-03T09:00", out, 50, ids()).unwrap();
    s.entries[1].entry.protected_at = Some(55);
    s.entries[2].approving = Some(55);
    assert_eq!(
        s.delete_before(60),
        1,
        "only the unprotected entry not being approved"
    );
    let keys: Vec<&str> = s.entries.iter().map(|e| e.entry.key.as_str()).collect();
    assert_eq!(keys, ["b", "c"]);
    s.running = Some(RoutineRun {
        occurrence: "2026-11-04T09:00".into(),
        started_at: 2,
        task: Some("t-000002".into()),
    });
    let next = validate(&stage(json!([briefing("a"), briefing("d")])), 10).unwrap();
    let mut fresh = ids();
    for _ in 0..3 {
        fresh();
    }
    assert_eq!(
        publish(&mut s, "t-000002", "2026-11-04T09:00", next, 70, fresh).unwrap(),
        1,
        "the deleted key a is skipped"
    );
}

#[test]
fn only_the_open_run_publishes() {
    let mut s = state_with_run("t-000002");
    let out = validate(&stage(json!([briefing("a")])), 10).unwrap();
    assert_eq!(
        publish(&mut s, "t-000001", "x", out, 1, ids()),
        Err(PublishError::NotCurrent("t-000001".into()))
    );
    assert!(s.entries.is_empty() && s.state.is_none());
}

#[test]
fn the_stage_must_be_a_regular_file_and_not_a_symlink() {
    let dir = std::env::temp_dir().join(format!(
        "muxa-stage-{}-{:08x}",
        std::process::id(),
        getrandom::u32().unwrap()
    ));
    std::fs::create_dir_all(dir.join(STAGE_DIR)).unwrap();
    assert!(read(&dir, 10).unwrap_err()[0].contains("does not exist"));
    std::fs::write(dir.join("elsewhere.json"), r#"{"version":1}"#).unwrap();
    std::os::unix::fs::symlink(dir.join("elsewhere.json"), dir.join(OUTPUT)).unwrap();
    assert!(read(&dir, 10).unwrap_err()[0].contains("symlink"));
    std::fs::remove_file(dir.join(OUTPUT)).unwrap();
    std::fs::write(dir.join(OUTPUT), r#"{"version":1}"#).unwrap();
    assert!(read(&dir, 10).is_ok());
    std::fs::remove_dir_all(dir).unwrap();
}
