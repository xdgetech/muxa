use serde_json::json;

use super::*;

fn request() -> AssistantRequest {
    serde_json::from_value(json!({
        "id": "q-0000000a", "work": "w-0000000b", "version": 3, "workspace": "lisk",
        "scope": "draft", "delivery": "pending", "deliveryAt": 1,
        "createdAt": 1, "updatedAt": 1,
        "results": [
            {"version": 2, "at": 1, "summary": "Second step.", "stale": true,
             "worker": "strategist", "step": 2,
             "notes": {"text": "notes-mark", "base": "sha"},
             "writes": [{"action": "Post to #release", "target": "slack:#release",
                         "content": "Ships Friday.", "hash": "sha256:a"}]},
            {"version": 2, "at": 1, "summary": "First step.", "stale": true, "step": 1},
        ],
    }))
    .unwrap()
}

#[test]
fn the_mark_finds_the_continued_result_and_a_new_version_clears_it() {
    let q = request();
    let mut extra = Extra::new();
    assert!(of(&q, &extra).is_none());
    let f = FollowUp {
        version: 2,
        step: 2,
        write: Some(0),
    };
    set(&mut extra, Some(&f));
    let (got, r) = of(&q, &extra).unwrap();
    assert_eq!((got, r.summary.as_str()), (f, "Second step."));
    set(&mut extra, None);
    assert!(get(&extra).is_none());
}

#[test]
fn the_goal_names_the_result_and_the_write_to_draft_again() {
    let q = request();
    let r = &q.results[0];
    let send = Brief::new(
        &FollowUp {
            version: 2,
            step: 2,
            write: None,
        },
        r,
    );
    let s = send.section();
    assert!(s.contains("version 2, step 2 by strategist,"), "{s}");
    assert!(s.contains(FILE) && !s.contains("new draft"), "{s}");
    let revise = Brief::new(
        &FollowUp {
            version: 2,
            step: 2,
            write: Some(0),
        },
        r,
    );
    let s = revise.section();
    assert!(
        s.contains("write 1, \"Post to #release\" to slack:#release"),
        "{s}"
    );
}
