use muxa_protocol::{CorrectionKind, Delivery};

use super::*;
use crate::assistant::corrections::Seen;
use crate::assistant::corrections::tests::{NOW, host};
use crate::assistant::policies::{self, Fields};

const KEPT: &str = "kept-mark-2b7d";
/// `work` shares nothing in this host's file.
const UNSHARED: &str = "unshared-mark-90ae";

#[test]
fn the_brief_carries_what_the_user_told_from_the_store_and_no_unshared_record() {
    let h = host();
    let now = crate::records::now();
    let add = |text: String, scope: &str| {
        let fields = Fields {
            text: Some(text),
            scope: Some(scope.into()),
            from: None,
            until: None,
        };
        policies::add(&h.config, fields, now).unwrap()
    };
    let shared = add(format!("Recruiting first {KEPT}"), "all");
    add(format!("Sales first {UNSHARED}"), "work");
    let current = h.plan(NOW, Delivery::Resulted, &[]);
    let seen = |workspace: Option<&str>, reason: &str| Seen {
        reason: Some(reason.into()),
        ..Seen::row(
            CorrectionKind::Dismissed,
            "w-00000001",
            workspace.map(Into::into),
        )
    };
    let mine = corrections::record(&h.config, &current, seen(None, KEPT), now).unwrap();
    corrections::record(&h.config, &current, seen(Some("work"), UNSHARED), now).unwrap();
    let pending = h.plan(NOW + 10, Delivery::Pending, &[]);
    let dir = h.config.state_dir.as_ref().unwrap().join("run");
    std::fs::create_dir_all(dir.join(".muxa/assistant")).unwrap();
    write(&h.config, &pending.id, &dir).unwrap();
    let text = std::fs::read_to_string(dir.join(BRIEF)).unwrap();
    let at = |needle: &str| {
        text.find(needle)
            .unwrap_or_else(|| panic!("{needle} in:\n{text}"))
    };
    assert!(at("## What the workspaces share") < at("## What the user told you"));
    assert!(at("## What the user told you") < at("## Your notes"));
    assert!(at(&shared.id) < at(&mine.id), "policies before corrections");
    assert!(!text.contains(UNSHARED), "{text}");
}
